//!
//! Example with a lot of animations using GridView
//!
//! Use with gtk 4.8 for better performance
//!
//! I need to fix cache clearing
//! also some of animations not playing
//!
use gtk::prelude::*;
use gtk_rlottie as rlt; // I suggest to rename this package in dependencies

const APP_ID: &str = "com.github.yuraiz.RltHello";

fn main() {
    let app = gtk::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &gtk::Application) {
    let stickers = std::path::Path::new("examples/animations");
    assert!(stickers.is_dir());

    let paths: Vec<_> = stickers
        .read_dir()
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .map(|p| p.to_str().unwrap().to_owned())
        .take(50)
        .collect();

    let vector: Vec<model::AnimationState> = (0..10_000)
        .into_iter()
        .map(|i| model::AnimationState::new(paths[i % paths.len()].clone()))
        .collect();

    let model = gtk::gio::ListStore::new(model::AnimationState::static_type());

    // Add the vector to the model
    model.extend_from_slice(&vector);

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_factory, list_item| {
        let animation = fixed_size::FixedSizeBin::new();
        list_item.set_child(Some(&animation));
    });

    factory.connect_bind(|_, list_item| {
        let model = list_item
            .item()
            .and_downcast::<model::AnimationState>()
            .unwrap();

        let bin = list_item
            .child()
            .and_downcast::<fixed_size::FixedSizeBin>()
            .unwrap();

        let animation = rlt::Animation::from_filename(&model.path());

        animation.set_halign(gtk::Align::Center);
        animation.set_loop(true);
        animation.play();

        bin.set_child(animation);
    });

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
    use std::cell::OnceCell;

    mod imp {

        use super::*;

        #[derive(Default)]
        pub struct AnimationState {
            pub(super) path: OnceCell<String>,
        }

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
        pub fn new(path: String) -> Self {
            let obj: Self = Object::new();
            obj.imp().path.set(path).unwrap();
            obj
        }

        pub fn path(&self) -> String {
            self.imp().path.get().unwrap().clone()
        }
    }
}

mod fixed_size {
    use super::*;
    use glib::Object;
    use gtk::glib;
    use std::cell::RefCell;

    mod imp {

        use super::*;

        #[derive(Default)]
        pub struct FixedSizeBin {
            pub(super) child: RefCell<Option<rlt::Animation>>,
        }

        #[glib::object_subclass]
        impl ObjectSubclass for FixedSizeBin {
            const NAME: &'static str = "MyGtkAppFixedSizeBin";
            type ParentType = gtk::Widget;
            type Type = super::FixedSizeBin;
        }

        impl ObjectImpl for FixedSizeBin {
            fn dispose(&self) {
                if let Some(child) = &*self.child.borrow() {
                    child.unparent();
                }
            }
        }

        impl WidgetImpl for FixedSizeBin {
            fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
                if let Some(child) = &*self.child.borrow() {
                    child.allocate(width, height, baseline, None);
                }
            }

            fn request_mode(&self) -> gtk::SizeRequestMode {
                gtk::SizeRequestMode::ConstantSize
            }

            fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
                (0, 16, -1, -1)
            }
        }
    }

    glib::wrapper! {
        pub struct FixedSizeBin(ObjectSubclass<imp::FixedSizeBin>)
            @extends gtk::Widget;
    }

    impl FixedSizeBin {
        pub fn new() -> Self {
            Object::new()
        }

        pub fn set_child(&self, child: rlt::Animation) {
            child.set_parent(self);
            if let Some(old_child) = self.imp().child.replace(Some(child)) {
                old_child.unparent();
            };

            self.queue_allocate();
            self.queue_draw();
        }
    }
}
