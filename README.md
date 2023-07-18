# party-saver
MIDI Translator for Xone:K2 -> Rekordbox

## Introduction
While Rekordbox has support for mapping non-pioneer MIDI controllers, their implementation of the MIDI standard is rather poor, and relies on quirks of their own controllers.
In particular, it gets confused by the distinction between the `NOTE ON` and `NOTE OFF` events, so this program converts all note events to `NOTE ON` events.

## Features
- Allows using the continuous encoders to emulate rotary potentiometer encoders.
- Allows using the continous encoders to emulate distinct buttons for clockwise/counter-clockwise motion.
- Allows multiplexing the rotary potentiometers for FX controls, and using switches for channel-masking.
- Allows multiplexing (and inverting) the fourth linear fader as a tempo fader, with deck-switching and soft pickup.

Most of the code is fairly specific to the layout defined in `rekordbox-mappings.csv`, and may require modification if a different mapping is used.
