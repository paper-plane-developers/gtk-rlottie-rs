use glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

use super::AnimationPaintable;

mod imp {
    use super::*;
    use glib::once_cell::sync::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct Animation {
        pub(super) animation: RefCell<Option<AnimationPaintable>>,
        pub(super) property_queue: RefCell<Vec<(String, glib::Value)>>,
        pub(super) default_size: Cell<(f64, f64)>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Animation {
        const NAME: &'static str = "LottieAnimation";
        type Type = super::Animation;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Animation {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            obj.connect_scale_factor_notify(|obj| {
                if let Some(ref animation) = &*obj.imp().animation.borrow() {
                    animation.set_scale_factor(obj.scale_factor() as f64);
                }
            });
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoolean::new(
                        "playing",
                        "Playing",
                        "Animation is playing",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "loop",
                        "Loop",
                        "Loop animation",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "use-cache",
                        "Use cache",
                        "Do not use cache for animations that plays rarely",
                        true,
                        glib::ParamFlags::WRITABLE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "reversed",
                        "Reversed",
                        "Reversed frame order",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecDouble::new(
                        "progress",
                        "Progress",
                        "Set progress of the animation",
                        0.0,
                        1.0,
                        0.0,
                        glib::ParamFlags::READWRITE,
                    ),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let name = pspec.name();
            if let Some(ref animation) = &*self.animation.borrow() {
                match name {
                    "playing" => animation.is_playing().to_value(),
                    "loop" => animation.is_loop().to_value(),
                    "reversed" => animation.is_reversed().to_value(),
                    "progress" => animation.progress().to_value(),
                    _ => unimplemented!(),
                }
            } else {
                // Try to find value in queue
                let properties = &*self.property_queue.borrow();

                let mut current_val: Option<glib::Value> = None;

                for property in properties {
                    if property.0.as_str() == name {
                        current_val = Some(property.1.to_owned());
                    }
                }

                if let Some(val) = current_val {
                    val
                } else {
                    // Default values
                    match name {
                        "playing" | "loop" | "reversed" => false.to_value(),
                        "progress" => 0.0.to_value(),
                        _ => unimplemented!(),
                    }
                }
            }
        }

        fn set_property(
            &self,
            _obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            if let Some(ref animation) = &*self.animation.borrow() {
                match pspec.name() {
                    "playing" => animation.set_playing(value.get().unwrap()),
                    "loop" => animation.set_loop(value.get().unwrap()),
                    "use-cache" => animation.set_use_cache(value.get().unwrap()),
                    "reversed" => animation.set_reversed(value.get().unwrap()),
                    "progress" => animation.set_progress(value.get().unwrap()),
                    _ => unimplemented!(),
                }
            } else {
                self.property_queue
                    .borrow_mut()
                    .push((pspec.name().to_owned(), value.to_owned()));
            }
        }
    }

    impl WidgetImpl for Animation {
        fn snapshot(&self, widget: &Self::Type, snapshot: &gtk::Snapshot) {
            if let Some(ref animation) = &*self.animation.borrow() {
                let width = widget.width();
                let height = widget.height();

                let aspect_ratio = animation.intrinsic_aspect_ratio();
                let widget_aspect_ratio = width as f64 / height as f64;

                let (width, height) = if aspect_ratio < widget_aspect_ratio {
                    (((height as f64) * aspect_ratio), height as f64)
                } else {
                    (width as f64, ((width as f64) / aspect_ratio))
                };

                animation.snapshot(snapshot.upcast_ref(), width, height);
            }
        }

        fn request_mode(&self, _widget: &Self::Type) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(
            &self,
            _widget: &Self::Type,
            orientation: gtk::Orientation,
            for_size: i32,
        ) -> (i32, i32, i32, i32) {
            if for_size < 0 {
                let default = self.default_size.get();
                match orientation {
                    gtk::Orientation::Horizontal => (0, default.0 as i32, -1, -1),
                    gtk::Orientation::Vertical => (0, default.1 as i32, -1, -1),
                    _ => unimplemented!(),
                }
            } else if let Some(ref animation) = &*self.animation.borrow() {
                let aspect_ratio = animation.intrinsic_aspect_ratio();
                match orientation {
                    gtk::Orientation::Vertical => {
                        // height
                        let size = (for_size as f64 * aspect_ratio) as i32;
                        (0, size, -1, -1)
                    }
                    gtk::Orientation::Horizontal => {
                        // width
                        let size = (for_size as f64 / aspect_ratio) as i32;
                        (0, size, -1, -1)
                    }
                    _ => unimplemented!(),
                }
            } else {
                (0, 0, -1, -1)
            }
        }
    }
}

glib::wrapper! {
    /// Widget that displays vector lottie animation
    pub struct Animation(ObjectSubclass<imp::Animation>)
        @extends gtk::Widget;
}

impl Animation {
    fn set_animation(&self, animation: AnimationPaintable) {
        animation.connect_invalidate_contents(clone!(@weak self as obj => move |_| {
            obj.queue_draw();
        }));

        self.imp().default_size.set(animation.size());

        for property in ["playing", "loop", "reversed", "progress"] {
            self.bind_property(property, &animation, property).build();
        }

        self.imp().animation.replace(Some(animation));

        let properties = self.imp().property_queue.take();
        for property in properties {
            self.set_property_from_value(&property.0, &property.1);
        }
    }

    /// Creates animation from json of tgs files.
    pub fn from_file(file: gio::File) -> Self {
        let obj: Self = glib::Object::new(&[]).expect("Failed to create LottieAnimation");
        // glib::Object::new(&[("file", &file)]).expect("Failed to create LottieAnimation");
        let animation = AnimationPaintable::from_file(file);
        obj.set_animation(animation);
        obj
    }

    /// Creates animation from json of tgs files from the given filename.
    pub fn from_filename(path: &str) -> Self {
        let file = gio::File::for_path(path);
        Self::from_file(file)
    }

    // /// Set to use the cache or not.
    // ///
    // /// By default animation have the cache
    // /// it uses ram to reduse cpu usage
    // ///
    // /// and you can disable it when animation
    // /// plays once and don't need a cache
    pub fn set_use_cache(&self, value: bool) {
        self.set_property("use-cache", value);
    }

    /// Reversed frame order.
    pub fn is_reversed(&self) -> bool {
        self.property("reversed")
    }

    /// Sets reversed or default frame order.
    pub fn set_reversed(&self, value: bool) {
        self.set_property("reversed", value);
    }

    /// Returns current progress.
    pub fn progress(&self) -> f64 {
        self.property("progress")
    }

    /// Sets current progress.
    pub fn set_progress(&self, value: f64) {
        self.set_property("progress", value);
    }

    // Media functions

    /// Play the animation.
    pub fn play(&self) {
        self.set_property("playing", true.to_value());
    }

    /// Pause the animation.
    pub fn pause(&self) {
        self.set_property("playing", false.to_value());
    }

    /// Returns whether the animation is set to loop.
    pub fn is_loop(&self) -> bool {
        self.property("loop")
    }

    /// Sets whether the animation should loop.
    pub fn set_loop(&self, loop_: bool) {
        self.set_property("loop", loop_.to_value());
    }
}
