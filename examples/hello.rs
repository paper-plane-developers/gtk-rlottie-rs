use gtk::prelude::*;
use gtk_rlottie as rlt; // I suggest to rename this package in dependencies

const APP_ID: &str = "com.github.yuraiz.RltHello";

fn main() {
    let app = gtk::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &gtk::Application) {
    // You can open either json or telegram stickers formats
    let hand_animation_path = "examples/animations/AuthorizationStateWaitRegistration.tgs";

    let animation = rlt::Animation::from_filename(hand_animation_path);
    animation.set_halign(gtk::Align::Center);
    animation.set_loop(true);
    animation.play();

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .width_request(200)
        .height_request(200)
        .child(&animation)
        .build();
    window.present();
}
