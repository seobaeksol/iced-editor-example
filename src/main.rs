use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use iced::highlighter;
use iced::keyboard;
use iced::keyboard::Key;
use iced::widget::{
    button, column, container, horizontal_space, pick_list, row, text, text_editor, tooltip, Space,
};
use iced::{Element, Font, Length, Settings, Task, Theme};
use iced_futures::{MaybeSend, Subscription};

// Let's see if it works!

fn main() -> iced::Result {
    iced::application(Editor::title, Editor::update, Editor::view)
        .theme(Editor::theme)
        .executor::<TokioExecutor>()
        .settings(Settings {
            default_font: Font::with_name("JetBrains Mono"),
            fonts: vec![
                include_bytes!("../fonts/editor-icon.ttf").as_slice().into(),
                include_bytes!("../fonts/JetBrainsMono-Regular.ttf")
                    .as_slice()
                    .into(),
            ],
            ..Settings::default()
        })
        .subscription(Editor::subscription)
        .run_with(Editor::initialize)
}

#[derive(Debug, Clone)]
enum Message {
    Edit(text_editor::Action),
    Open,
    New,
    Save,
    FileOpened(Result<(PathBuf, Arc<String>), Error>),
    FileSaved(Result<PathBuf, Error>),
    ThemeSelected(highlighter::Theme),
}

struct Editor {
    path: Option<PathBuf>,
    content: text_editor::Content,
    error: Option<Error>,
    theme: highlighter::Theme,
    is_modified: bool,
}

impl Editor {
    fn new() -> Self {
        Self {
            content: text_editor::Content::default(),
            error: None,
            path: None,
            theme: highlighter::Theme::SolarizedDark,
            is_modified: false,
        }
    }

    fn initialize() -> (Self, Task<Message>) {
        (
            Self::new(),
            Task::perform(load_file(default_file()), Message::FileOpened),
        )
    }

    fn title(&self) -> String {
        String::from("A cool editor!")
    }

    fn theme(&self) -> Theme {
        if self.theme.is_dark() {
            Theme::Dark
        } else {
            Theme::Light
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Edit(action) => {
                if action.is_edit() {
                    self.is_modified = true;
                }

                self.content.perform(action);
                Task::none()
            }
            Message::FileOpened(Ok((path, content))) => {
                self.path = Some(path);
                self.content = text_editor::Content::with_text(&content);
                Task::none()
            }
            Message::FileOpened(Err(error)) => {
                self.error = Some(error);
                Task::none()
            }
            Message::Open => Task::perform(pick_file(), Message::FileOpened),
            Message::New => {
                self.path = None;
                self.content = text_editor::Content::new();
                self.is_modified = true;

                Task::none()
            }
            Message::Save => {
                let contents = self.content.text();

                Task::perform(save_file(self.path.clone(), contents), Message::FileSaved)
            }
            Message::FileSaved(Ok(path)) => {
                self.path = Some(path);
                self.is_modified = false;

                Task::none()
            }
            Message::FileSaved(Err(error)) => {
                self.error = Some(error);

                Task::none()
            }
            Message::ThemeSelected(theme) => {
                self.theme = theme;
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let controls = row![
            action(new_icon(), "New", Some(Message::New)),
            action(open_icon(), "Open", Some(Message::Open)),
            action(
                save_icon(),
                "Save",
                self.is_modified.then_some(Message::Save)
            ),
            horizontal_space(),
            pick_list(
                highlighter::Theme::ALL,
                Some(self.theme),
                Message::ThemeSelected
            ),
        ]
        .spacing(10);

        let input = text_editor(&self.content)
            .on_action(Message::Edit)
            .height(Length::Fill)
            .highlight(
                self.path
                    .as_ref()
                    .and_then(|path| path.extension()?.to_str())
                    .unwrap_or("rs"),
                self.theme,
            );

        let status_bar = {
            let file_path = if let Some(Error::IOFailed(error)) = self.error.as_ref() {
                text(error.to_string())
            } else {
                match self.path.as_deref().map(Path::to_str) {
                    Some(Some(path)) => text(path).size(14),
                    None => text("New File"),
                    _ => text(""),
                }
            };

            let error_msg = if let Some(error) = &self.error {
                let error_text = match error {
                    Error::DialogClosed => String::from("DialogClosed"),
                    Error::IOFailed(err_kind) => format!("IO Error: {}", err_kind.to_string()),
                };

                text(error_text)
            } else {
                text("")
            };

            let position = {
                let (line, column) = self.content.cursor_position();
                text(format!("{}:{}", line + 1, column + 1))
            };

            row![
                file_path,
                horizontal_space(),
                error_msg,
                Space::with_width(10),
                position
            ]
        };

        let body = column![controls, input, status_bar].spacing(5);

        container(body).padding(10).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        keyboard::on_key_press(|key_code, modifiers| match key_code {
            Key::Character(c) if c == "s" && modifiers.control() => Some(Message::Save),
            _ => None,
        })
    }
}

async fn load_file(path: PathBuf) -> Result<(PathBuf, Arc<String>), Error> {
    let content = tokio::fs::read_to_string(&path)
        .await
        .map(Arc::new)
        .map_err(|error| error.kind())
        .map_err(Error::IOFailed)?;

    Ok((path, content))
}

async fn pick_file() -> Result<(PathBuf, Arc<String>), Error> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Choose a text file...")
        .pick_file()
        .await
        .ok_or(Error::DialogClosed)?;

    load_file(handle.path().to_owned()).await
}

async fn save_file(path: Option<PathBuf>, text: String) -> Result<PathBuf, Error> {
    let path = if let Some(path) = path {
        path
    } else {
        rfd::AsyncFileDialog::new()
            .set_title("Choose a file name...")
            .save_file()
            .await
            .ok_or(Error::DialogClosed)
            .map(|handle| handle.path().to_owned())?
    };

    tokio::fs::write(&path, text)
        .await
        .map_err(|error| Error::IOFailed(error.kind()))?;

    Ok(path)
}

fn default_file() -> PathBuf {
    PathBuf::from(format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR")))
}

#[derive(Debug, Clone)]
enum Error {
    DialogClosed,
    IOFailed(io::ErrorKind),
}

struct TokioExecutor(tokio::runtime::Runtime);

impl iced::Executor for TokioExecutor {
    fn new() -> Result<Self, io::Error>
    where
        Self: Sized,
    {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map(Self)
    }

    fn spawn(&self, future: impl std::future::Future<Output = ()> + MaybeSend + 'static) {
        let _ = tokio::runtime::Runtime::spawn(&self.0, future);
    }

    fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = tokio::runtime::Runtime::enter(&self.0);
        f()
    }
}

fn action<'a>(
    content: Element<'a, Message>,
    label: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let is_modified = on_press.is_some();
    tooltip(
        button(container(content).center_x(30))
            .on_press_maybe(on_press)
            .padding([5, 10])
            .style(move |theme, status| {
                if is_modified {
                    button::primary(theme, status)
                } else {
                    button::secondary(theme, status)
                }
            }),
        label,
        tooltip::Position::FollowCursor,
    )
    .into()
}

fn new_icon<'a>() -> Element<'a, Message> {
    icon('\u{e800}')
}

fn save_icon<'a>() -> Element<'a, Message> {
    icon('\u{e801}')
}

fn open_icon<'a>() -> Element<'a, Message> {
    icon('\u{f115}')
}
fn icon<'a>(codepoint: char) -> Element<'a, Message> {
    const ICON_FONT: Font = Font::with_name("editor-icon");

    text(codepoint).font(ICON_FONT).into()
}
