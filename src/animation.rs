use glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib};

use std::time::Duration;

use crate::cache::AnimationEntry;

use glib::once_cell::sync::*;
use std::{
    cell::Cell,
    sync::{Arc, Mutex},
};

mod imp {
    use super::*;

    #[derive(Default, Debug)]
    pub struct Animation {
        pub(super) cache_entry: OnceCell<Arc<Mutex<AnimationEntry>>>,

        pub(super) frame_start: Cell<i64>,

        pub(super) frame_num: Cell<usize>,
        pub(super) frame_delay: Cell<Duration>,
        pub(super) totalframe: Cell<usize>,

        pub(super) default_size: Cell<(i32, i32)>,
        pub(super) size: Cell<(f64, f64)>,

        // fields for properties
        pub(super) loop_: Cell<bool>,
        pub(super) playing: Cell<bool>,
        pub(super) reversed: Cell<bool>,
        pub(super) use_cache: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Animation {
        const NAME: &'static str = "LottieAnimation";
        type Type = super::Animation;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Animation {
        fn constructed(&self) {
            self.parent_constructed();
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoolean::builder("loop").build(),
                    glib::ParamSpecBoolean::builder("playing").build(),
                    glib::ParamSpecDouble::builder("progress")
                        .minimum(0.0)
                        .maximum(1.0)
                        .build(),
                    glib::ParamSpecBoolean::builder("reversed").build(),
                    glib::ParamSpecBoolean::builder("use-cache").build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "loop" => self.loop_.get().to_value(),
                "playing" => self.playing.get().to_value(),
                "progress" => {
                    (self.frame_num.get() as f64 / (self.totalframe.get() - 1) as f64).to_value()
                }
                "reversed" => self.reversed.get().to_value(),
                "use-cache" => self.use_cache.get().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "loop" => self.loop_.set(value.get().unwrap()),
                "playing" => {
                    self.playing.set(value.get().unwrap());
                    let frame_time = (glib::monotonic_time() * 6) / 100000;
                    let frame_start = frame_time - self.frame_num.get() as i64;
                    self.frame_start.set(frame_start);
                    self.obj().queue_draw();
                }
                "progress" => {
                    let progress: f64 = value.get().unwrap();
                    let frame_num = ((self.totalframe.get() - 1) as f64 * progress) as usize;
                    self.obj().setup_frame(frame_num);
                }
                "reversed" => self.reversed.set(value.get().unwrap()),
                "use-cache" => {
                    // TODO: remove that
                }
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for Animation {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();

            let width = widget.width();
            let height = widget.height();

            let aspect_ratio = {
                let (width, height) = self.default_size.get();
                width as f64 / height as f64
            };

            let widget_aspect_ratio = width as f64 / height as f64;

            let (width, height) = if aspect_ratio < widget_aspect_ratio {
                (((height as f64) * aspect_ratio), height as f64)
            } else {
                (width as f64, ((width as f64) / aspect_ratio))
            };

            self.resize(width, height);

            let index = self.frame_num.get();

            let cache_entry = widget.lock_cache_entry();

            if let Some(texture) =
                cache_entry.frame_immediate(width as usize, height as usize, index)
            {
                texture.snapshot(snapshot, width, height);
            } else if let Some(texture) =
                cache_entry.nearest_frame_immediate(width as usize, height as usize, index)
            {
                texture.snapshot(snapshot, width, height);
            } else {
                // dbg!("no texture");
            }
        }

        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let aspect_ratio = {
                let (width, height) = self.default_size.get();
                width as f64 / height as f64
            };

            if for_size < 0 {
                let (width, height) = self.default_size.get();
                return match orientation {
                    gtk::Orientation::Horizontal => (0, width, -1, -1),
                    gtk::Orientation::Vertical => (0, height, -1, -1),
                    _ => unimplemented!(),
                };
            };

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
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
        }
    }

    impl Animation {
        fn resize(&self, width: f64, height: f64) {
            let aspect_ratio = width / height;

            let (width, height) = if aspect_ratio <= 1.0 {
                // width is smaller
                (width, ((height) / aspect_ratio))
            } else {
                // height is smaller
                (((width) / aspect_ratio), height)
            };

            if self.size.get() != (width, height) {
                self.size.set((width, height));
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
    fn tick(&self, clock: &gdk::FrameClock) -> Continue {
        let imp = self.imp();

        if self.is_mapped() && self.is_playing() {
            let totalframe = self.imp().totalframe.get();
            let reversed = self.imp().reversed.get();

            let frame =
                ((clock.frame_time() * 6) / 100000 - imp.frame_start.get()) as usize % totalframe;

            let prev_frame = imp.frame_num.get();

            if frame != prev_frame {
                if reversed {
                    self.setup_frame(totalframe - frame - 1);
                } else {
                    self.setup_frame(frame);
                }
            }

            if frame == totalframe - 1 && !self.is_loop() {
                self.pause();
            }
        }

        Continue(true)
    }

    fn setup_frame(&self, frame_num: usize) {
        let imp = self.imp();

        let (width, height) = imp.size.get();
        let scale_factor = self.scale_factor() as f64;
        let width = (width * scale_factor) as usize;
        let height = (height * scale_factor) as usize;

        // let cache_entry = self.lock_cache_entry();

        // if cache_entry.frame_immediate(width, height, index).is_some();

        // let (sender, receiver) = glib::MainContext::channel(Default::default());

        // receiver.attach(
        //     None,
        //     clone!(@weak self as obj => @default-return Continue(false), move |_| {
        //         Continue(false)
        //     }),
        // );

        self.lock_cache_entry()
            .request_frame(width, height, frame_num, move |_texture| {
                // sender.send(()).unwrap()
            });

        self.request_draw(frame_num);
    }

    fn lock_cache_entry(&self) -> std::sync::MutexGuard<'_, AnimationEntry> {
        self.imp().cache_entry.get().unwrap().lock().unwrap()
    }

    pub fn request_draw(&self, frame_num: usize) {
        self.imp().frame_num.set(frame_num);
        self.queue_draw();
    }

    pub fn open(&self, file: gio::File) {
        let path = file.path().unwrap();
        let path = path.to_str().unwrap().to_owned();

        let (sender, receiver) =
            glib::MainContext::channel::<Arc<Mutex<AnimationEntry>>>(Default::default());

        receiver.attach(
            None,
            clone!(@weak self as obj => @default-return Continue(false), move |entry| {
                let imp = obj.imp();

                let info = entry.lock().unwrap().info();

                imp.cache_entry.set(entry).unwrap();

                imp.frame_num.set(0);
                imp.frame_delay.set(info.frame_delay);
                imp.totalframe.set(info.totalframe);

                let (width, height) = info.default_size;
                imp.size.set((width as f64, height as f64));
                imp.default_size.set(info.default_size);

                imp.obj().setup_frame(0);
                imp.obj().add_tick_callback(Self::tick);

                Continue(false)
            }),
        );

        crate::cache::open_animation(&path, move |entry| sender.send(entry).unwrap());
    }

    /// Creates animation from json of tgs files.
    pub fn from_file(file: &impl IsA<gio::File>) -> Self {
        let obj: Self = glib::Object::new();
        obj.open(file.to_owned().upcast());
        obj
    }

    /// Creates animation from json of tgs files from the given filename.
    pub fn from_filename(path: &str) -> Self {
        let file = gio::File::for_path(path);
        Self::from_file(&file)
    }

    /// Return whether the animation is currently using cache.
    pub fn use_cache(&self, value: bool) {
        self.set_property("use-cache", value);
    }

    /// Set to use the cache or not.
    ///
    /// By default animation have the cache
    /// it uses ram to reduse cpu usage
    ///
    /// and you can disable it when animation
    /// plays once and don't need a cache
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

    /// Return whether the animation is currently playing.
    pub fn is_playing(&self) -> bool {
        self.imp().playing.get()
    }

    /// Play the animation.
    pub fn play(&self) {
        self.set_property("playing", true);
    }

    /// Pause the animation.
    pub fn pause(&self) {
        self.set_property("playing", false);
    }

    /// Returns whether the animation is set to loop.
    pub fn is_loop(&self) -> bool {
        self.property("loop")
    }

    /// Sets whether the animation should loop.
    pub fn set_loop(&self, loop_: bool) {
        self.set_property("loop", loop_);
    }
}
