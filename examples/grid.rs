/**
 * Example with a lot of animations that I want to optimize
 *
 * You can check grid_view example for better performance
 */
use gtk::prelude::*;
use gtk_rlottie as rlt; // I suggest to rename this package in dependencies

const APP_ID: &str = "com.github.yuraiz.RltHello";

fn main() {
    let app = gtk::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &gtk::Application) {
    fn create_animation() -> rlt::Animation {
        let hand_animation_path = "examples/animations/AuthorizationStateWaitRegistration.tgs";

        let animation = rlt::Animation::from_filename(hand_animation_path);
        animation.set_halign(gtk::Align::Center);
        animation.set_loop(true);
        animation.set_height_request(50);
        animation.set_width_request(50);
        animation.play();
        animation
    }

    let grid = gtk::Grid::new();

    let scrolled_window = gtk::ScrolledWindow::builder().child(&grid).build();

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .width_request(200)
        .height_request(200)
        .child(&scrolled_window)
        .build();

    for top in 0..20 {
        for left in 0..10 {
            {
                grid.attach(&create_animation(), left as _, top as _, 1, 1)
            }
        }
    }
    window.present();
}
