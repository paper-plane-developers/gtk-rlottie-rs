use glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib};

mod imp {
    use super::*;
    use flate2::read::GzDecoder;
    use glib::once_cell::sync::Lazy;
    use std::cell::{Cell, RefCell};
    use std::io::Read;

    #[derive(Default)]
    pub struct AnimationPaintable {
        animation: RefCell<Option<rlottie::Animation>>,
        frame_num: Cell<usize>,
        frame_delay: Cell<f64>,
        totalframe: Cell<usize>,
        cache: RefCell<Vec<Option<gdk::MemoryTexture>>>,
        last_cache_use: Cell<Option<std::time::Instant>>,
        cache_is_out_of_date: Cell<bool>,
        aspect_ratio: Cell<f64>,
        size: Cell<(f64, f64)>,

        scale_factor: Cell<f64>,

        // fields for properties
        use_cache: Cell<bool>,
        reversed: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AnimationPaintable {
        const NAME: &'static str = "LottieAnimationPaintable";
        type Type = super::AnimationPaintable;
        type ParentType = gtk::MediaStream;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for AnimationPaintable {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);
            self.use_cache.set(true);
            self.scale_factor.set(1.0);
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
                "progress" => {
                    let progress: f64 = value.get().unwrap();
                    let frame_num = ((self.totalframe.get() - 1) as f64 * progress) as usize;
                    self.frame_num.set(frame_num);

                    self.setup_next_frame();
                    obj.invalidate_contents();
                }
                _ => unimplemented!(),
            }
        }
    }

    impl MediaStreamImpl for AnimationPaintable {
        fn play(&self, media_stream: &Self::Type) -> bool {
            media_stream.invalidate_contents();
            true
        }

        fn pause(&self, _: &Self::Type) {
            // hide warning
        }
    }

    impl gdk::subclass::paintable::PaintableImpl for AnimationPaintable {
        fn intrinsic_aspect_ratio(&self, _paintable: &Self::Type) -> f64 {
            self.aspect_ratio.get()
        }

        fn snapshot(&self, obj: &Self::Type, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            self.resize(width, height);

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
                // total_frame - 1
                1
            };

            if frame_num != last || obj.is_loop() {
                glib::timeout_add_once(
                    std::time::Duration::from_secs_f64(self.frame_delay.get()),
                    clone!(@weak obj =>  move || {

                        let imp = obj.imp();

                        if imp.cache_is_out_of_date.get() {
                             imp.cache.replace(vec![None; imp.totalframe.get()]);
                        }

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
                std::thread::spawn(clone!(@weak obj =>  move || {
                    let imp = obj.imp();
                    obj.pause();
                    if imp.cache_is_out_of_date.take() {
                        imp.cache.replace(vec![None; imp.totalframe.get()]);
                    }
                    imp.frame_num.set(first);
                    imp.setup_next_frame();
                    obj.invalidate_contents();
                }));
            }
        }
    }

    impl AnimationPaintable {
        fn resize(&self, width: f64, height: f64) {
            let aspect_ratio = width as f64 / height as f64;

            let (width, height) = if aspect_ratio <= 1.0 {
                // width is smaller
                (width as f64, ((height as f64) / aspect_ratio))
            } else {
                // height is smaller
                (((width as f64) / aspect_ratio), height as f64)
            };

            if self.size.get() != (width, height) {
                self.size.set((width, height));

                self.cache_is_out_of_date.set(true);
            }
        }

        pub(super) fn scale(&self, scale_factor: f64) {
            self.scale_factor.set(scale_factor);
            self.cache_is_out_of_date.set(true);
        }

        pub(super) fn open(&self, file: gio::File) {
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
                                .expect("LottieAnimationPaintable: unsupporded file type")
                        } else {
                            unimplemented!("LottieAnimationPaintable: unsupporded file type")
                        }
                    }
                }
            };

            self.frame_num.set(0);

            self.frame_delay.set(1.0 / animation.framerate() as f64);
            let totalframe = animation.totalframe();
            let size = animation.size();
            self.totalframe.set(totalframe);
            self.animation.replace(Some(animation));

            self.size.set((size.width as f64, size.height as f64));
            self.aspect_ratio
                .set(size.width as f64 / size.height as f64);

            let cache_size = if self.use_cache.get() { totalframe } else { 1 };

            self.cache.replace(vec![None; cache_size]);
        }

        fn setup_next_frame(&self) {
            let mut cache = self.cache.borrow_mut();
            let frame_num = self.frame_num.get();

            if cache[frame_num].is_none() {
                if let Some(ref mut animation) = *self.animation.borrow_mut() {
                    let (width, height) = self.size.get();

                    let scale_factor = self.scale_factor.get();

                    let (width, height) = {
                        (
                            (width * scale_factor) as i32,
                            (height * scale_factor) as i32,
                        )
                    };

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
    }
}

glib::wrapper! {
    /// Implementation of [gtk::MediaMediaStream](https://docs.gtk.org/gtk4/class.MediaStream.html) for lottie.
    ///
    /// Example of usage
    /// ```
    /// let lottie_animation = LottieAnimationPaintable::from_file(file);
    ///
    /// lottie_animation.play();
    /// lottie_animation.set_loop(true);
    ///
    /// picture.set_paintable(Some(&lottie_animation));
    /// ```
    pub struct AnimationPaintable(ObjectSubclass<imp::AnimationPaintable>)
        @extends gtk::MediaStream,
        @implements gdk::Paintable;
}

impl AnimationPaintable {
    /// Creates animation from json of tgs files.
    pub fn from_file(file: gio::File) -> Self {
        let obj: Self = glib::Object::new(&[]).expect("Failed to create LottieAnimationPaintable");
        obj.imp().open(file);
        obj
    }

    /// Creates animation from json of tgs files from the given filename.
    pub fn from_filename(path: &str) -> Self {
        let file = gio::File::for_path(path);
        Self::from_file(file)
    }

    pub fn set_scale_factor(&self, scale_factor: f64) {
        self.imp().scale(scale_factor);
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
}

unsafe impl Sync for AnimationPaintable {}
unsafe impl Send for AnimationPaintable {}
