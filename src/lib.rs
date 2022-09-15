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
    pub struct LottieAnimation {
        animation: RefCell<Option<rlottie::Animation>>,
        frame_num: Cell<usize>,
        frame_delay: Cell<f64>,
        totalframe: Cell<usize>,
        intrinsic: Cell<(i32, i32, f64)>,
        cache: RefCell<Vec<gdk::MemoryTexture>>,
        last_cache_use: Cell<Option<std::time::Instant>>,

        pub(super) size: Cell<(i32, i32)>,

        // fields for properties
        use_cache: Cell<bool>,
        width: Cell<i32>,
        height: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LottieAnimation {
        const NAME: &'static str = "ContentLottieAnimation";
        type Type = super::LottieAnimation;
        type ParentType = gtk::MediaFile;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for LottieAnimation {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);
            self.use_cache.set(true);
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    // glib::ParamSpecU
                    glib::ParamSpecBoolean::new(
                        "use-cache",
                        "Use cache",
                        "Do not use cache for animations that plays rarely",
                        true,
                        glib::ParamFlags::WRITABLE,
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
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "use-cache" => self.use_cache.get().to_value(),
                "width" => self.width.get().to_value(),
                "height" => self.height.get().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(
            &self,
            _obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "use-cache" => {
                    let use_cache = value.get().unwrap();
                    self.use_cache.set(use_cache);
                    if use_cache {
                        self.cache.take();
                        self.frame_num.set(0);
                    } else {
                        self.cache.borrow_mut().truncate(1);
                    }
                    dbg!(value);
                }
                "width" => {
                    self.width.set(value.get().unwrap());
                    self.update_size()
                }
                "height" => {
                    self.height.set(value.get().unwrap());
                    self.update_size()
                }
                _ => unimplemented!(),
            }
        }
    }

    impl MediaFileImpl for LottieAnimation {
        fn open(&self, media_file: &Self::Type) {
            if let Some(file) = media_file.file() {
                let path = file.path().unwrap();
                let animation = match path
                    .extension()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap_or_default()
                {
                    "json" => rlottie::Animation::from_file(path)
                        .expect("LottieAnimation: can't open animation"),
                    "tgs" => {
                        let data = file.load_contents(gio::Cancellable::NONE).unwrap().0;

                        let mut gz = GzDecoder::new(&*data);

                        let mut buf = String::new();

                        gz.read_to_string(&mut buf).expect("can't read file");

                        rlottie::Animation::from_data(
                            buf,
                            path.file_name().unwrap().to_str().unwrap(),
                            "",
                        )
                        .expect("LottieAnimation: create tgs animation")
                    }
                    _ => panic!("LottieAnimation: unsupporded file type"),
                };

                let was_playing = media_file.is_playing();
                media_file.pause();

                self.frame_num.set(0);

                self.frame_delay.set(1.0 / animation.framerate() as f64);
                self.totalframe.set(animation.totalframe());
                self.animation.replace(Some(animation));

                self.update_size();

                if was_playing {
                    media_file.play();
                }
            }
        }
    }
    impl MediaStreamImpl for LottieAnimation {
        fn play(&self, media_stream: &Self::Type) -> bool {
            media_stream.invalidate_contents();
            true
        }

        fn pause(&self, _: &Self::Type) {
            // hide warning
        }
    }

    impl gdk::subclass::paintable::PaintableImpl for LottieAnimation {
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
            let frame_num = (self.frame_num.get() + total_frame - 1) % total_frame;

            let cache_index = if self.use_cache.get() { frame_num } else { 0 };

            let cache = self.cache.borrow_mut();

            if let Some(texture) = &cache.get(cache_index) {
                texture.snapshot(snapshot, width, height);
                self.last_cache_use.set(Some(std::time::Instant::now()));
            }

            if obj.is_playing() && (frame_num != (total_frame - 2) || obj.is_loop()) {
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
                                    dbg!(imp.cache.borrow_mut().truncate(1));
                                    obj.imp().frame_num.set(0);
                                }
                            }
                        }),
                    );
                }
            }
        }
    }

    impl LottieAnimation {
        fn setup_next_frame(&self) {
            let mut cache = self.cache.borrow_mut();
            let frame_num = self.frame_num.get();

            if cache.len() != self.totalframe.get() {
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

                    if self.use_cache.get() || frame_num == 0 {
                        cache.push(texture);
                    } else {
                        cache[0] = texture;
                    }
                }
            }
            self.frame_num.set((frame_num + 1) % self.totalframe.get());
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
    pub struct LottieAnimation(ObjectSubclass<imp::LottieAnimation>)
        @extends gtk::MediaFile, gtk::MediaStream,
        @implements gdk::Paintable;
}

impl LottieAnimation {
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
    pub fn set_use_cache(&self, val: bool) {
        self.set_property("use-cache", val);
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
}

unsafe impl Sync for LottieAnimation {}
unsafe impl Send for LottieAnimation {}
