/**
 * Example with a lot of animations using GridView
 *
 * I need to fix cache clearing
 * also some of animations not playing
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

    // let grid = gtk::Grid::new();

    let vector: Vec<model::AnimationState> = (0..=10000)
        .into_iter()
        .map(|_| model::AnimationState::new())
        .collect();

    let model = gtk::gio::ListStore::new(model::AnimationState::static_type());

    // Add the vector to the model
    model.extend_from_slice(&vector);

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_factory, list_item| {
        let animation = create_animation();
        list_item.set_child(Some(&animation));
    });

    let selection_model = gtk::NoSelection::new(Some(&model));

    let grid_view = gtk::GridView::new(Some(&selection_model), Some(&factory));

    grid_view.set_hscroll_policy(gtk::ScrollablePolicy::Natural);
    grid_view.set_vscroll_policy(gtk::ScrollablePolicy::Natural);

    grid_view.set_can_target(false);

    let scrolled_window = gtk::ScrolledWindow::builder().child(&grid_view).build();

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .width_request(200)
        .height_request(200)
        .child(&scrolled_window)
        .build();

    // for top in 0..20 {
    //     for left in 0..10 {
    //         {
    //             grid.attach(&create_animation(), left as _, top as _, 1, 1)
    //         }
    //     }
    // }
    window.present();
}

mod model {

    use glib::Object;
    use gtk::glib;

    mod imp {
        use super::*;
        use gtk::subclass::prelude::*;

        #[derive(Default)]
        pub struct AnimationState;

        #[glib::object_subclass]
        impl ObjectSubclass for AnimationState {
            const NAME: &'static str = "MyGtkAppAnimationState";
            type Type = super::AnimationState;
        }

        impl ObjectImpl for AnimationState {}
    }

    glib::wrapper! {
        pub struct AnimationState(ObjectSubclass<imp::AnimationState>);
    }

    impl AnimationState {
        pub fn new() -> Self {
            Object::new(&[]).expect("Failed to create `AnimationState`.")
        }
    }
}
