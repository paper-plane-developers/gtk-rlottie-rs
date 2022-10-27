use glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib};

use flate2::read::GzDecoder;
use std::io::Read;
use std::time::Duration;

struct RenderInfo {
    frame_num: usize,
    width: i32,
    height: i32,
    sender: glib::Sender<(usize, gdk::MemoryTexture)>,
}

mod imp {
    use super::*;
    use glib::once_cell::sync::Lazy;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct AnimationPaintable {
        pub(super) render_sender: RefCell<Option<std::sync::mpsc::Sender<RenderInfo>>>,

        pub(super) frame_num: Cell<usize>,
        pub(super) frame_delay: Cell<Duration>,
        pub(super) totalframe: Cell<usize>,
        pub(super) cache: RefCell<Vec<Option<gdk::MemoryTexture>>>,
        pub(super) last_cache_use: Cell<Option<std::time::Instant>>,
        pub(super) cache_is_out_of_date: Cell<bool>,
        pub(super) default_size: Cell<(i32, i32)>,
        pub(super) size: Cell<(f64, f64)>,

        pub(super) scale_factor: Cell<f64>,

        // fields for properties
        pub(super) use_cache: Cell<bool>,
        pub(super) reversed: Cell<bool>,
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
                    glib::ParamSpecBoolean::builder("reversed").build(),
                    glib::ParamSpecBoolean::builder("use-cache").build(),
                    glib::ParamSpecDouble::builder("progress")
                        .minimum(0.0)
                        .maximum(1.0)
                        .build(),
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

                    self.setup_next_frame(obj);
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
        fn intrinsic_width(&self, _paintable: &Self::Type) -> i32 {
            self.default_size.get().1
        }

        fn intrinsic_height(&self, _paintable: &Self::Type) -> i32 {
            self.default_size.get().1
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
                1
            } else {
                total_frame - 1
            };

            if frame_num == last && !obj.is_loop() {
                let first = if self.reversed.get() {
                    total_frame - 1
                } else {
                    1
                };
                self.frame_num.set(first);
                obj.pause();
            }

            if obj.is_playing() && (frame_num != last || obj.is_loop()) {
                glib::timeout_add_local_once(
                    self.frame_delay.get(),
                    clone!(@weak obj =>  move || {
                        obj.imp().setup_next_frame(&obj);
                    }),
                );
            } else {
                self.setup_next_frame(obj);
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

        pub(super) fn setup_next_frame(&self, obj: &super::AnimationPaintable) {
            if let Ok(cache) = self.cache.try_borrow() {
                let frame_num = self.frame_num.get();

                fn next_frame(obj: &super::AnimationPaintable, frame_num: usize) {
                    let imp = obj.imp();

                    let total_frame = imp.totalframe.get();
                    let shift = if imp.reversed.get() {
                        total_frame - 1
                    } else {
                        1
                    };

                    imp.frame_num.set((frame_num + shift) % total_frame);
                    obj.invalidate_contents();
                }

                if cache[frame_num].is_none() || self.cache_is_out_of_date.get() {
                    let (sender, receiver) = glib::MainContext::channel::<(
                        usize,
                        gdk::MemoryTexture,
                    )>(Default::default());

                    receiver.attach(
                        None,
                        clone!(@weak obj => @default-return glib::Continue(false), move |(frame_num, texture)| {
                            let imp = obj.imp();

                            if imp.cache_is_out_of_date.take() {
                                imp.cache.replace(vec![None; imp.totalframe.get()]);
                            }

                            let index = if imp.use_cache.get() { frame_num } else { 0 };

                            imp.cache.borrow_mut()[index] = Some(texture);

                            next_frame(&obj, frame_num);

                            glib::Continue(false)
                        }),
                    );

                    if let Some(ref render_sender) = *self.render_sender.borrow() {
                        let (width, height) = self.size.get();
                        let scale_factor = self.scale_factor.get();
                        let width = (width * scale_factor) as i32;
                        let height = (height * scale_factor) as i32;

                        let render_info = RenderInfo {
                            frame_num,
                            width,
                            height,
                            sender,
                        };

                        render_sender.send(render_info).unwrap();
                    }
                } else {
                    next_frame(obj, frame_num);
                }
            }
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
    pub(super) fn open(&self, file: gio::File) {
        struct AnimationInfo {
            totalframe: usize,
            default_size: (i32, i32),
            frame_delay: Duration,
            duration: Duration,
        }

        let (sender, receiver) =
            glib::MainContext::sync_channel::<AnimationInfo>(Default::default(), 0);

        receiver.attach(
            None,
            clone!(@weak self as obj => @default-return glib::Continue(false), move |animation_info| {
                let AnimationInfo { frame_delay, totalframe, default_size, duration } = animation_info;

                let imp = obj.imp();

                imp.frame_num.set(0);
                imp.frame_delay.set(frame_delay);
                imp.totalframe.set(totalframe);

                let (width, height) = default_size;
                imp.size.set((width as f64, height as f64));
                imp.default_size
                    .set(default_size);

                    let cache_size = if imp.use_cache.get() { totalframe } else { 1 };
                    imp.cache.replace(vec![None; cache_size]);

                    let cache_cleaner = {
                    clone!(@weak obj => @default-return glib::Continue(false),  move || {
                            let imp = obj.imp();
                            if let Some(instant) = imp.last_cache_use.get() {
                                let elapsed = instant.elapsed();
                                if elapsed > duration * 2 && elapsed < duration * 4 {
                                    println!("Cache cleaned");
                                    imp.cache.replace(vec![None; imp.totalframe.get()]);
                                }
                            }
                        glib::Continue(true)
                    })
                };

                glib::timeout_add_local(
                    duration,
                    cache_cleaner,
                );

                glib::Continue(false)
            }),
        );

        let (render_sender, render_receiver) = std::sync::mpsc::channel::<RenderInfo>();

        self.imp().render_sender.replace(Some(render_sender));

        std::thread::spawn(move || {
            let path = file.path().expect("file not found");

            let cache_key = path.file_name().unwrap().to_str().unwrap().to_owned();

            let mut animation = {
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

            let size = animation.size();

            let animation_info = AnimationInfo {
                frame_delay: Duration::from_secs_f64(1.0 / animation.framerate() as f64),
                totalframe: animation.totalframe(),
                default_size: (size.width as i32, size.height as i32),
                duration: Duration::from_secs_f64(animation.duration()),
            };

            sender.send(animation_info).unwrap();

            while let Ok(render_info) = render_receiver.recv() {
                let RenderInfo {
                    frame_num,
                    width,
                    height,
                    sender,
                } = render_info;

                let size = rlottie::Size::new(width as usize, height as usize);
                let mut surface = rlottie::Surface::new(size);

                animation.render(frame_num, &mut surface);

                let data = glib::Bytes::from_owned(surface);

                let texture = gdk::MemoryTexture::new(
                    width,
                    height,
                    gdk::MemoryFormat::B8g8r8a8,
                    &data,
                    width as usize * 4,
                );

                sender.send((frame_num, texture)).unwrap();
            }
        });
    }

    /// Creates animation from json of tgs files.
    pub fn from_file(file: gio::File) -> Self {
        let obj: Self = glib::Object::new(&[]).expect("Failed to create LottieAnimationPaintable");
        obj.open(file);
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
