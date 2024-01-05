use rodio::source::{Buffered, Source};
use rodio::{Decoder, OutputStream};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use iced::widget::{button, column, container, row, text, Checkbox, Radio, Slider};
use iced::{executor, time, Alignment, Application, Command, Element, Length, Settings, Theme};
use iced_futures::Subscription;

fn main() -> iced::Result {
    Metronome::run(Settings::default())
}
struct Metronome {
    value: u32,
    state: MetronomeState,
    player_thread: Sender<Beat>,
    subdivision: Option<Subdivision>,
    is_set_to_quack: bool,
    is_timer_on: bool,
    timer: Timer,
}

#[derive(Clone, Copy)]
struct Timer {
    mins: u32,
    secs: u32,
}

impl Default for Timer {
    fn default() -> Self {
        Self { mins: 2, secs: 30 }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Subdivision {
    Quarter,
    Eighth,
}

struct MetronomeSettings {
    value: u32,
    is_set_to_quack: bool,
    is_timer_on: bool,
    subdivision: Option<Subdivision>,
    timer: Timer,
}

impl Default for MetronomeSettings {
    fn default() -> Self {
        Self {
            value: 120,
            is_set_to_quack: false,
            is_timer_on: false,
            subdivision: Some(Subdivision::Quarter),
            timer: Timer::default(),
        }
    }
}

#[derive(PartialEq)]
enum MetronomeState {
    Stopped,
    Play,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    Toggle,
    Beat,
    IncrementBPM,
    DecrementBPM,
    SlideChangeBPM(u32),
    ChangeSubdivision(Subdivision),
    ToggleQuack(bool),
    Tick,
    ToggleTimer(bool),
}

enum Beat {
    Beat,
    Quack,
}

impl Application for Metronome {
    type Executor = executor::Default;
    type Flags = MetronomeSettings;
    type Message = Message;
    type Theme = Theme;

    fn new(flags: MetronomeSettings) -> (Metronome, Command<Self::Message>) {
        let sound_sources = read_sounds_into_buffer();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || player_thread(sound_sources, rx));
        (
            Self {
                value: flags.value,
                state: MetronomeState::Stopped,
                player_thread: tx,
                subdivision: flags.subdivision,
                is_set_to_quack: flags.is_set_to_quack,
                is_timer_on: flags.is_timer_on,
                timer: flags.timer,
            },
            Command::none(),
        )
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        // This factor will determine how fast to play the click
        let subdivision_factor = match self.subdivision.unwrap() {
            Subdivision::Quarter => 1.,
            Subdivision::Eighth => 2.,
        };

        let mut subscriptions: Vec<Subscription<_>> = vec![];
        let click_sub = time::every(Duration::from_secs_f64(
            60. / self.value as f64 / subdivision_factor,
        ))
        .map(|_| Message::Beat);
        let timer_sub = time::every(Duration::from_secs(1)).map(|_| Message::Tick);

        subscriptions.push(click_sub);
        // Timer should only tick if the checkbox is toggled
        if self.is_timer_on {
            subscriptions.push(timer_sub);
        }

        match self.state {
            MetronomeState::Play => Subscription::batch(subscriptions),
            MetronomeState::Stopped => iced::Subscription::none(),
        }
    }

    fn title(&self) -> String {
        String::from("Metronome")
    }

    fn view(&self) -> Element<Message> {
        let content = column![
            text(self.value).size(100),
            row![
                button(container("-").width(Length::Fill).center_x())
                    .on_press(Message::DecrementBPM)
                    .width(35),
                button(container("+").width(Length::Fill).center_x())
                    .on_press(Message::IncrementBPM)
                    .width(35)
            ]
            .spacing(20),
            row![Slider::new(40..=200, self.value, Message::SlideChangeBPM)
                .width(Length::Fixed(350.0))],
            button(
                container(if self.state == MetronomeState::Play {
                    "Stop"
                } else {
                    "Play"
                })
                .width(Length::Fill)
                .center_x()
            )
            .on_press(Message::Toggle)
            .width(75),
            row![
                Radio::new(
                    "Quarter Note",
                    Subdivision::Quarter,
                    self.subdivision,
                    Message::ChangeSubdivision
                ),
                Radio::new(
                    "Eighth Note",
                    Subdivision::Eighth,
                    self.subdivision,
                    Message::ChangeSubdivision
                )
            ]
            .spacing(20),
            row![
                Checkbox::new("Timer ", self.is_timer_on, Message::ToggleTimer),
                text(format_mins_and_secs(self.timer.mins, self.timer.secs))
            ],
            row![Checkbox::new(
                if !self.is_set_to_quack {
                    "Set to Quack"
                } else {
                    "Set to Click"
                },
                self.is_set_to_quack,
                Message::ToggleQuack
            )]
        ]
        .spacing(10)
        .padding(20)
        .align_items(Alignment::Center);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }

    fn update(&mut self, message: Message) -> Command<Self::Message> {
        match message {
            Message::IncrementBPM => {
                self.value += 1;
                Command::none()
            }
            Message::DecrementBPM => {
                self.value -= 1;
                Command::none()
            }
            Message::Toggle => {
                if self.state == MetronomeState::Stopped {
                    println!("Playing metronome!");
                    self.state = MetronomeState::Play;
                    Command::perform(async {}, |()| Message::Beat)
                } else {
                    println!("Stopping Metronome");
                    self.state = MetronomeState::Stopped;
                    Command::none()
                }
            }
            Message::Beat => {
                match self.state {
                    MetronomeState::Play => {
                        if self.is_set_to_quack {
                            self.player_thread.send(Beat::Quack).unwrap();
                        } else {
                            self.player_thread.send(Beat::Beat).unwrap();
                        }
                    }
                    MetronomeState::Stopped => {}
                };
                Command::none()
            }
            Message::SlideChangeBPM(bpm) => {
                self.value = bpm;
                Command::none()
            }
            Message::ChangeSubdivision(subdivision) => {
                self.subdivision = Some(subdivision);
                Command::none()
            }
            Message::ToggleQuack(should_quack) => {
                self.is_set_to_quack = should_quack;
                Command::none()
            }
            Message::Tick => {
                // Timer should not overflow into negative seconds
                if (self.timer.mins * 60 + self.timer.secs) > 0 {
                    self.timer = update_timer(self.timer);
                }
                Command::none()
            }
            Message::ToggleTimer(toggle_timer) => {
                self.is_timer_on = toggle_timer;
                Command::none()
            }
        }
    }
}

fn format_mins_and_secs(mins: u32, secs: u32) -> String {
    if secs < 10 {
        format!("{}:0{}", mins, secs)
    } else {
        format!("{}:{}", mins, secs)
    }
}

fn update_timer(mut timer: Timer) -> Timer {
    if timer.secs == 0 && timer.mins > 0 {
        timer.mins -= 1;
        timer.secs = 59;
    } else {
        timer.secs -= 1;
    }
    timer
}

fn player_thread(
    sound_sources: HashMap<String, Buffered<Decoder<BufReader<File>>>>,
    rx: Receiver<Beat>,
) {
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    while let Ok(beat) = rx.recv() {
        match beat {
            Beat::Beat => {
                let click_sound = sound_sources.get(&"click".to_string()).unwrap();
                stream_handle
                    .play_raw(click_sound.clone().convert_samples())
                    .unwrap();
            }
            Beat::Quack => {
                let quack_sound = sound_sources.get(&"quack".to_string()).unwrap();
                stream_handle
                    .play_raw(quack_sound.clone().convert_samples())
                    .unwrap();
            }
        }
    }
}
fn read_sounds_into_buffer() -> HashMap<String, Buffered<Decoder<BufReader<File>>>> {
    let mut sound_sources: HashMap<String, Buffered<Decoder<BufReader<File>>>> = HashMap::new();

    let click_file = BufReader::new(
        File::open(
            "/Users/Mattdamachine/Code/Rust/practice/rodio_practice/rust_metronome/media/strong_beat.wav",
        )
        .unwrap(),
    );
    let click_source = Decoder::new(click_file).unwrap().buffered();

    sound_sources.insert("click".to_string(), click_source);

    let quack_file = BufReader::new(
        File::open(
            "/Users/Mattdamachine/Code/Rust/practice/rodio_practice/rust_metronome/media/duck_quack.wav",
        )
        .unwrap(),
    );

    let quack_source = Decoder::new(quack_file).unwrap().buffered();

    sound_sources.insert("quack".to_string(), quack_source);

    sound_sources
}
