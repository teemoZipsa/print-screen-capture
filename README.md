# Print Screen Capture

A small Windows utility that listens for the `Print Screen` key and saves a full virtual-desktop screenshot as a PNG file.

Screenshots are saved to:

```text
%USERPROFILE%\Pictures\PrintScreenCapture
```

## Build

```powershell
cargo build --release
```

## Run

```powershell
cargo run --release
```

While it is running:

- Press `Print Screen` to save a screenshot.
- Press `Ctrl+C` in the terminal to quit.

To capture once and exit:

```powershell
cargo run --release -- --capture-once
```

To check whether the `Print Screen` global hotkey is available:

```powershell
cargo run --release -- --check-hotkey
```

## Notes

- This is Windows-only.
- If another app already owns the `Print Screen` global hotkey, startup will fail with a Windows error.

<!-- Repository metadata maintenance note. -->
