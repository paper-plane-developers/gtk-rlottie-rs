# gtk-rlottie-rs

gtk Widget that renders lottie animations and telegram stickers using rlottie

use `cargo run --example hello` to run example

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

rlottie for fedora

```sh
sudo dnf install rlottie-devel
```

Animations for examples are taken from the [Unigram repo](https://github.com/UnigramDev/Unigram/tree/develop/Unigram/Unigram/Assets/Animations)
