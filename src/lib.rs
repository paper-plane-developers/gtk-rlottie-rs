use glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib};

mod imp {
    use super::*;
    use flate2::read::GzDecoder;
    use glib::once_cell::sync::Lazy;
    use rlottie;
    use std::io::Read;
    use std::{
        cell::{Cell, RefCell},
        collections::VecDeque,
    };

    #[derive(Default)]
    pub struct Animation {
        animation: RefCell<Option<rlottie::Animation>>,
        frame_num: Cell<usize>,
        frame_delay: Cell<f64>,
        totalframe: Cell<usize>,
        cache: RefCell<Vec<Option<gdk::MemoryTexture>>>,
        last_cache_use: Cell<Option<std::time::Instant>>,
        first_play: Cell<bool>,

        pub(super) size: Cell<(i32, i32)>,

        // fields for properties
        use_cache: Cell<bool>,
        reversed: Cell<bool>,
        width: Cell<i32>,
        height: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Animation {
        const NAME: &'static str = "LottieAnimation";
        type Type = super::Animation;
        type ParentType = gtk::MediaFile;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for Animation {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);
            self.use_cache.set(true);
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoolean::new(
                        "use-cache",
                        "Use cache",
                        "Do not use cache for animations that plays rarely",
                        true,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecBoolean::new(
                        "reversed",
                        "Reversed",
                        "Reversed frame order",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecInt::new(
                        "width",
                        "Preffered width",
                        "Width to use for render animation",
                        0,
                        i32::MAX,
                        0,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecInt::new(
                        "height",
                        "Preffered height",
                        "Height to use for render animation",
                        0,
                        i32::MAX,
                        0,
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
            match pspec.name() {
                "use-cache" => self.use_cache.get().to_value(),
                "reversed" => self.reversed.get().to_value(),
                "width" => self.width.get().to_value(),
                "height" => self.height.get().to_value(),
                "progress" => {
                    (self.frame_num.get() as f64 / (self.totalframe.get() - 1) as f64).to_value()
                }
                _ => unimplemented!(),
            }
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "use-cache" => {
                    let use_cache = value.get().unwrap();
                    self.use_cache.set(use_cache);
                    self.cache.replace(vec![None; self.totalframe.get()]);
                }
                "reversed" => {
                    self.reversed.set(value.get().unwrap());
                }
                "width" => {
                    self.width.set(value.get().unwrap());
                    self.update_size()
                }
                "height" => {
                    self.height.set(value.get().unwrap());
                    self.update_size()
                }
                "progress" => {
                    let progress: f64 = value.get().unwrap();
                    let frame_num = ((self.totalframe.get() - 1) as f64 * progress) as usize;
                    self.frame_num.set(frame_num);
                    obj.invalidate_contents();
                }
                _ => unimplemented!(),
            }
        }
    }

    impl MediaFileImpl for Animation {
        fn open(&self, media_file: &Self::Type) {
            if let Some(file) = media_file.file() {
                let path = file.path().expect("file not found");
                let cache_key = path.file_name().unwrap().to_str().unwrap().to_owned();

                let animation = {
                    match rlottie::Animation::from_file(path) {
                        Some(animation) => animation,
                        _ => {
                            let data = file.load_contents(gio::Cancellable::NONE).unwrap().0;

                            let mut gz = GzDecoder::new(&*data);
                            let mut buf = String::new();

                            if gz.read_to_string(&mut buf).is_ok() {
                                rlottie::Animation::from_data(buf, cache_key, "")
                                    .expect("LottieAnimation: unsupporded file type")
                            } else {
                                unimplemented!("LottieAnimation: unsupporded file type")
                            }
                        }
                    }
                };

                let was_playing = media_file.is_playing();
                media_file.pause();

                self.frame_num.set(0);

                self.frame_delay.set(1.0 / animation.framerate() as f64);
                let totalframe = animation.totalframe();
                self.totalframe.set(totalframe);
                self.animation.replace(Some(animation));

                let cache_size = if self.use_cache.get() { totalframe } else { 1 };

                self.cache.replace(vec![None; cache_size]);

                self.update_size();

                if was_playing {
                    media_file.play();
                }
            }
        }
    }
    impl MediaStreamImpl for Animation {
        fn play(&self, media_stream: &Self::Type) -> bool {
            let lp = media_stream.is_loop();
            media_stream.set_loop(true);
            media_stream.invalidate_contents();
            media_stream.set_loop(lp);
            true
        }

        fn pause(&self, _: &Self::Type) {
            // hide warning
        }
    }

    impl gdk::subclass::paintable::PaintableImpl for Animation {
        fn flags(&self, _: &Self::Type) -> gdk::PaintableFlags {
            gdk::PaintableFlags::SIZE
        }

        fn intrinsic_width(&self, _: &Self::Type) -> i32 {
            self.width.get() as i32
        }

        fn intrinsic_height(&self, _: &Self::Type) -> i32 {
            self.height.get() as i32
        }

        fn snapshot(&self, obj: &Self::Type, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            let total_frame = self.totalframe.get();
            let shift = if self.reversed.get() {
                1
            } else {
                total_frame - 1
            };

            let frame_num = (self.frame_num.get() + shift) % total_frame;

            let cache_index = if self.use_cache.get() { frame_num } else { 0 };

            let cache = self.cache.borrow_mut();

            if let Some(texture) = &cache[cache_index] {
                texture.snapshot(snapshot, width, height);
                self.last_cache_use.set(Some(std::time::Instant::now()));
            }

            let last = if self.reversed.get() {
                total_frame - 1
            } else {
                total_frame - 1
                // 1
            };

            if obj.is_playing() {
                if frame_num != last || obj.is_loop() {
                    glib::timeout_add_once(
                        std::time::Duration::from_secs_f64(self.frame_delay.get()),
                        clone!(@weak obj =>  move || {
                            obj.imp().setup_next_frame();
                            obj.invalidate_contents();
                        }),
                    );

                    if self.use_cache.get() && frame_num == 0 {
                        glib::timeout_add_local_once(
                            std::time::Duration::from_secs(2),
                            clone!(@weak obj =>  move || {
                                    let imp = obj.imp();
                                    if let Some(instatnt) = imp.last_cache_use.get() {
                                        if instatnt.elapsed() > std::time::Duration::from_secs_f32(0.5) {
                                        imp.cache.replace(vec![None; imp.totalframe.get()]);
                                    }
                                }
                            }),
                        );
                    }
                } else {
                    let first = if self.reversed.get() {
                        total_frame - 1
                    } else {
                        1
                    };
                    obj.pause();
                    self.frame_num.set(first);
                    obj.invalidate_contents();
                }
            }
        }
    }

    impl Animation {
        fn setup_next_frame(&self) {
            let mut cache = self.cache.borrow_mut();
            let frame_num = self.frame_num.get();

            if cache[frame_num].is_none() {
                if let Some(ref mut animation) = *self.animation.borrow_mut() {
                    let (width, height) = self.size.get();

                    let mut surface =
                        rlottie::Surface::new(rlottie::Size::new(width as usize, height as usize));
                    animation.render(frame_num, &mut surface);

                    let data = surface.data();
                    let data = unsafe {
                        std::slice::from_raw_parts_mut(data.as_ptr() as *mut u8, data.len() * 4)
                    };
                    let data = glib::Bytes::from_owned(data.to_owned());

                    let texture = gdk::MemoryTexture::new(
                        width,
                        height,
                        gdk::MemoryFormat::B8g8r8a8,
                        &data,
                        width as usize * 4,
                    );

                    if self.use_cache.get() {
                        cache[frame_num] = Some(texture);
                    } else {
                        cache[0] = Some(texture);
                    }
                }
            }

            let total_frame = self.totalframe.get();
            let shift = if self.reversed.get() {
                total_frame - 1
            } else {
                1
            };

            self.frame_num.set((frame_num + shift) % total_frame);
        }

        fn update_size(&self) {
            if let Some(animation) = &*self.animation.borrow() {
                let animation_size = animation.size();
                let aspect_ratio = animation_size.width as f64 / animation_size.height as f64;
                let width = self.width.get();
                let height = self.height.get();

                let size = match (width, height) {
                    (0, 0) => (animation_size.width as i32, animation_size.height as i32),
                    (0, height) => ((height as f64 * aspect_ratio) as i32, height),
                    (width, 0) => (width, (width as f64 / aspect_ratio) as i32),
                    size => size,
                };

                self.size.set(size);
            }
        }
    }
}

glib::wrapper! {
    /// Implementation of [gtk::MediaFile](https://docs.gtk.org/gtk4/class.MediaFile.html) for lottie.
    ///
    /// Example of usage
    /// ```
    /// let lottie_animation = LottieAnimation::from_file(file);
    ///
    /// lottie_animation.play();
    /// lottie_animation.set_loop(true);
    ///
    /// picture.set_paintable(Some(&lottie_animation));
    /// ```
    pub struct Animation(ObjectSubclass<imp::Animation>)
        @extends gtk::MediaFile, gtk::MediaStream,
        @implements gdk::Paintable;
}

impl Animation {
    /// Creates animation from json of tgs files.
    pub fn from_file(file: gio::File) -> Self {
        glib::Object::new(&[("file", &file)]).expect("Failed to create LottieAnimation")
    }

    /// Creates animation from json of tgs files from the given filename.
    pub fn from_filename(path: &str) -> Self {
        let file = gio::File::for_path(path);
        Self::from_file(file)
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

    /// Returns current width and height of the animation.
    pub fn size(&self) -> (i32, i32) {
        self.imp().size.get()
    }

    /// Sets the size of the animation.
    ///
    /// you can set value to 0 and it will be selected depending on the animation
    pub fn set_size(&self, width: i32, height: i32) {
        self.set_properties(&[("width", &width.to_value()), ("height", &height.to_value())])
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
}

unsafe impl Sync for Animation {}
unsafe impl Send for Animation {}
