/**
 * Example with a lot of animations using GridView
 *
 * Use with gtk 4.8 for better performance
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
        animation.play();
        animation
    }

    let vector: Vec<model::AnimationState> = (0..100_000)
        .into_iter()
        .map(|_| model::AnimationState::new())
        .collect();

    let model = gtk::gio::ListStore::new(model::AnimationState::static_type());

    // Add the vector to the model
    model.extend_from_slice(&vector);

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_factory, list_item| {
        let animation = create_animation();
        let animation = fixed_size::FixedSizeBin::new(animation);
        list_item.set_child(Some(&animation));
    });

    factory.connect_bind(|_, _| {});

    let selection_model = gtk::NoSelection::new(Some(model));

    let grid_view = gtk::GridView::new(Some(selection_model), Some(factory));

    grid_view.set_hscroll_policy(gtk::ScrollablePolicy::Natural);
    grid_view.set_vscroll_policy(gtk::ScrollablePolicy::Natural);
    grid_view.set_max_columns(32);
    grid_view.set_can_target(false);

    let scrolled_window = gtk::ScrolledWindow::builder().child(&grid_view).build();

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .width_request(200)
        .height_request(200)
        .child(&scrolled_window)
        .build();

    window.present();
}

use gtk::subclass::prelude::*;

mod model {
    use super::*;
    use glib::Object;
    use gtk::glib;

    mod imp {
        use super::*;

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
            Object::new()
        }
    }
}

mod fixed_size {
    use super::*;
    use glib::Object;
    use gtk::glib;

    mod imp {
        use gtk::glib::once_cell::sync::OnceCell;

        use super::*;

        #[derive(Default)]
        pub struct FixedSizeBin(pub(super) OnceCell<rlt::Animation>);

        #[glib::object_subclass]
        impl ObjectSubclass for FixedSizeBin {
            const NAME: &'static str = "MyGtkAppFixedSizeBin";
            type ParentType = gtk::Widget;
            type Type = super::FixedSizeBin;
        }

        impl ObjectImpl for FixedSizeBin {}
        impl WidgetImpl for FixedSizeBin {
            fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
                self.0
                    .get()
                    .unwrap()
                    .allocate(width, height, baseline, None);
            }

            fn request_mode(&self) -> gtk::SizeRequestMode {
                gtk::SizeRequestMode::ConstantSize
            }

            fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
                (0, 30, -1, -1)
            }
        }

        impl Drop for FixedSizeBin {
            fn drop(&mut self) {
                self.0.get().unwrap().unparent();
            }
        }
    }

    glib::wrapper! {
        pub struct FixedSizeBin(ObjectSubclass<imp::FixedSizeBin>)
            @extends gtk::Widget;
    }

    impl FixedSizeBin {
        pub fn new(animation: rlt::Animation) -> Self {
            let obj: Self = Object::new();
            animation.set_parent(&obj);
            obj.imp().0.set(animation).unwrap();
            obj
        }
    }
}
