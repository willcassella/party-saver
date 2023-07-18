# party-saver
MIDI Translator for Xone:K2 -> Rekordbox

## Introduction
While Rekordbox has support for mapping non-pioneer MIDI controllers, their implementation of the MIDI standard is rather poor, and relies on quirks of their own controllers.
In particular, it gets confused by the distinction between the `NOTE ON` and `NOTE OFF` events, so this program converts all note events to `NOTE ON` events, among a few other features.

## Features
- Allows using the continuous encoders to emulate rotary potentiometer encoders.
- Allows using the continous encoders to emulate distinct buttons for clockwise/counter-clockwise motion.
- Allows multiplexing the rotary potentiometers for FX controls, and using switches for channel-masking.
- Allows multiplexing (and inverting) the fourth linear fader as a tempo fader, with deck-switching and soft pickup.

Most of the code is fairly specific to the layout defined in `rekordbox-mappings.csv`, and may require modification if a different mapping is used.

## Setup
1. Build and run with `cargo run --release`.
2. Follow the prompt to select the MIDI port of a connected Xone:K2.
3. Select "PartySaver" as your MIDI device in Rekordbox, and import the mappings from `rekordbox-mappings.csv`.
