use iced::widget::{button, column, text};
use iced::{Element, Task};

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("Grocery POS")
        .run()
}

#[derive(Default)]
struct App {
    screen: Screen,
}

#[derive(Default)]
enum Screen {
    #[default]
    Catalog,
    About,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    Catalog,
    About,
}

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        self.screen = match message {
            Message::Catalog => Screen::Catalog,
            Message::About => Screen::About,
        };
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let content = match self.screen {
            Screen::Catalog => "Product catalog is being prepared.",
            Screen::About => "Grocery POS is in development.",
        };
        column![
            button("Catalog").on_press(Message::Catalog),
            button("About").on_press(Message::About),
            text(content),
        ]
        .spacing(16)
        .padding(24)
        .into()
    }
}
