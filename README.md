# gtk-rlottie-rs

gtk::MediaFile that renders lottie animations and telegram stickers using rlottie

to use this library you need [rlottie](https://github.com/melix99/rlottie)

rlottie as flatpak module:

```json
{
    "name": "rlottie",
    "buildsystem": "meson",
    "config-opts": ["-Dwerror=false"],
    "sources": [
        {
            "type": "git",
            "url": "https://github.com/melix99/rlottie",
            "branch": "fix-build"
        }
    ]
}
```

example will be available soon
