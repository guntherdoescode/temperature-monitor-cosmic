mod app;
mod sensors;

use cosmic::iced::Limits;

fn main() -> cosmic::iced::Result {
    let settings = cosmic::app::Settings::default()
        .size_limits(Limits::NONE.min_width(560.0).min_height(420.0));
    cosmic::app::run::<app::AppModel>(settings, ())
}
