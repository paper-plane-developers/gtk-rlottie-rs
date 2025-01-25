use std::cell::Cell;
use std::cell::RefCell;
use std::io::Read;
use std::sync::OnceLock;
use std::time::Duration;

use flate2::read::GzDecoder;
use glib::clone;
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

#[derive(Debug)]
struct RenderInfo {
    frame_num: usize,
    width: i32,
    height: i32,
    sender: async_channel::Sender<(usize, gdk::MemoryTexture)>,
}

mod imp {
    use super::*;

    #[derive(Default, Debug)]
    pub struct Animation {
        pub(super) frame_start: Cell<i64>,

        pub(super) render_sender: RefCell<Option<std::sync::mpsc::Sender<RenderInfo>>>,
        pub(super) frame_num: Cell<usize>,
        pub(super) frame_delay: Cell<Duration>,
        pub(super) totalframe: Cell<usize>,
        pub(super) cache: RefCell<Vec<Option<gdk::MemoryTexture>>>,
        pub(super) last_cache_use: Cell<Option<std::time::Instant>>,
        pub(super) cache_is_out_of_date: Cell<bool>,
        pub(super) cache_dropped: Cell<bool>,
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

            self.obj().connect_scale_factor_notify(|obj| {
                obj.imp().cache_is_out_of_date.set(true);
            });
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: OnceLock<Vec<glib::ParamSpec>> = OnceLock::new();
            PROPERTIES.get_or_init(|| {
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
            })
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
                    let use_cache = value.get().unwrap();
                    if use_cache != self.use_cache.replace(use_cache) {
                        self.drop_cache();
                    }
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

            let index = if self.use_cache.get() {
                self.frame_num.get()
            } else {
                0
            };

            let cache = self.cache.borrow_mut();

            if cache.len() == 0 {
                return;
            }

            if let Some(texture) = &cache[index] {
                texture.snapshot(snapshot, width, height);
                self.last_cache_use.set(Some(std::time::Instant::now()));
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
    }

    impl Animation {
        pub fn drop_cache(&self) {
            if self.cache_dropped.replace(true) {
                return;
            }

            let mut cache = self.cache.borrow_mut();

            if cache.len() == 0 {
                return;
            }

            let index = self.frame_num.get();

            let current = cache[index].take();

            for frame in &mut *cache {
                *frame = None;
            }

            cache[index] = current;
        }

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

                self.cache_is_out_of_date.set(true);
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
    fn tick(&self, clock: &gdk::FrameClock) -> glib::ControlFlow {
        let imp = self.imp();

        if imp.use_cache.get() && !imp.cache_dropped.get() {
            if let Some(instant) = imp.last_cache_use.get() {
                let elapsed = instant.elapsed();
                if elapsed.as_secs() > 1 {
                    imp.drop_cache();
                    return glib::ControlFlow::Continue;
                }
            }
        }

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

        glib::ControlFlow::Continue
    }

    fn setup_frame(&self, frame_num: usize) {
        let imp = self.imp();

        let cache_is_out_of_date = imp.cache_is_out_of_date.get();
        let use_cache = imp.use_cache.get();

        let ignore_cache = cache_is_out_of_date || !use_cache;

        if let Ok(cache) = imp.cache.try_borrow() {
            if ignore_cache || cache[frame_num].is_none() {
                let (sender, receiver) = async_channel::unbounded::<(usize, gdk::MemoryTexture)>();

                glib::spawn_future_local(clone!(@to-owned imp => async move {
                    if let Ok((frame_num, texture)) = receiver.recv().await {
                        if imp.cache_is_out_of_date.take() {
                            imp.cache.replace(vec![None; imp.totalframe.get()]);
                        }

                        let index = if imp.use_cache.get() { frame_num } else { 0 };
                        imp.cache.borrow_mut()[index] = Some(texture);
                        imp.obj().request_draw(index);
                        imp.cache_dropped.set(false);
                    }
                }));

                if let Some(ref render_sender) = *imp.render_sender.borrow() {
                    let (width, height) = imp.size.get();
                    let scale_factor = self.scale_factor() as f64;
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
                self.request_draw(frame_num);
            }
        }
    }

    pub fn request_draw(&self, frame_num: usize) {
        self.imp().frame_num.set(frame_num);
        self.queue_draw();
    }

    pub fn open(&self, file: gio::File) {
        struct AnimationInfo {
            totalframe: usize,
            default_size: (i32, i32),
            frame_delay: Duration,
        }

        let (sender, receiver) = async_channel::unbounded::<AnimationInfo>();

        glib::spawn_future_local(clone!(@weak self as obj => async move {
            if let Ok(animation_info) = receiver.recv().await {
                let imp = obj.imp();

                let AnimationInfo { totalframe, default_size, frame_delay} = animation_info;

                imp.frame_num.set(0);
                imp.frame_delay.set(frame_delay);
                imp.totalframe.set(totalframe);

                let (width, height) = default_size;
                imp.size.set((width as f64, height as f64));
                imp.default_size
                    .set(default_size);

                imp.cache.replace(vec![None; totalframe]);
                imp.cache_dropped.set(true);

                imp.obj().setup_frame(0);
                imp.obj().add_tick_callback(Self::tick);
            }
        }));

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
                frame_delay: Duration::from_secs_f64(1.0 / animation.framerate()),
                totalframe: animation.totalframe(),
                default_size: (size.width as i32, size.height as i32),
            };

            glib::spawn_future(clone!(#[strong] sender, async move {
                _ = sender.send(animation_info).await;
            }));

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

                glib::spawn_future(clone!(@strong sender => async move {
                    _ = sender.send((frame_num, texture)).await;
                }));
            }
        });
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
