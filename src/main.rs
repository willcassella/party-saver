use midir::os::unix::{VirtualInput, VirtualOutput};
use midir::{MidiIO, MidiInput, MidiOutput, MidiOutputConnection};
use std::error::Error;
use std::io::{stdin, stdout, Write};

// This channel is reserved for actual messages from the device, or messages which haven't been
// substantially altered.
const DEVICE_CHANNEL: u8 = 0xF;

const FAKE_BUTTON_UP_CHANNEL: u8 = 0xE;
const FAKE_BUTTON_DOWN_CHANNEL: u8 = 0xD;
const FILTER_ENCODER_CHANNEL: u8 = 0xC;
const TEMPO_ENCODER_CHANNEL:u8 = 0xB;

const NOTE_OFF: u8 = 0x80;
const NOTE_ON: u8 = 0x90;
const CONTROL_CHANGE: u8 = 0xB0;

const HEADPHONE_MIX_CC: u8 = 20;
const HEADPHONE_VOLUME_CC: u8 = 21;
const DECK1_LOOP_CC: u8 = 1;
const DECK2_LOOP_CC: u8 = 2;
const DECK3_LOOP_CC: u8 = 0;
const MASTER_VOLUME_CC: u8 = 3;

// Inputs for filter controls.
const FILTER_CC: u8 = 15;
const DECK1_FILTER_TOGGLE_NOTE: u8 = 0x29;
const DECK2_FILTER_TOGGLE_NOTE: u8 = 0x2A;
const DECK3_FILTER_TOGGLE_NOTE: u8 = 0x28;

// Fake outputs for filter controls.
const DECK1_FILTER_CC: u8 = 1;
const DECK2_FILTER_CC: u8 = 2;
const DECK3_FILTER_CC: u8 = 0;

// Inputs for tempo controls.
const TEMPO_CC: u8 = 19;
const DECK1_TEMPO_TOGGLE_NOTE: u8 = 0x23;
const DECK2_TEMPO_TOGGLE_NOTE: u8 = 0x1F;
const DECK3_TEMPO_TOGGLE_NOTE: u8 = 0x27;

// Fake outputs for tempo controls.
const DECK1_TEMPO_CC: u8 = 1;
const DECK2_TEMPO_CC: u8 = 2;
const DECK3_TEMPO_CC: u8 = 0;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn log_send(
    channel: u8,
    kind: u8,
    code: u8,
    data: u8,
    out: &mut MidiOutputConnection,
) -> Result<()> {
    let message = [channel | kind, code, data];
    out.send(&message)?;
    println!("PartySaver->Rekordbox: {:?}", message);
    Ok(())
}

// Allows treating rotary encoders as pot encoders.
struct FakePotEncoder {
    value: u8,
}

impl FakePotEncoder {
    fn add(&mut self, data: u8) {
        let delta = match data {
            127 => -3,
            1 => 3,
            _ => {
                println!("Unknown data value: {}", data);
                0
            }
        };
        self.value = self.value.saturating_add_signed(delta).min(127);
    }

    fn send(&self, cc: u8, out: &mut MidiOutputConnection) -> Result<()> {
        log_send(DEVICE_CHANNEL, CONTROL_CHANGE, cc, self.value, out)
    }
}

impl Default for FakePotEncoder {
    fn default() -> Self {
        Self { value: 63 }
    }
}

// Rekordbox expects the same signal for on AND off for buttons for some stupid reason.
fn handle_button(note: u8, data: u8, out: &mut MidiOutputConnection) -> Result<()> {
    log_send(DEVICE_CHANNEL, NOTE_ON, note, data, out)
}

// Allows treating rotary encoders as buttons.
fn handle_fake_button(cc: u8, data: u8, out: &mut MidiOutputConnection) -> Result<()> {
    let channel = match data {
        1 => FAKE_BUTTON_UP_CHANNEL,
        127 => FAKE_BUTTON_DOWN_CHANNEL,
        _ => {
            println!("Unknown data value: {}", data);
            return Ok(());
        }
    };

    log_send(channel, NOTE_ON, cc, 127, out)
}

// Specialized control for the filter encoder.
struct FilterEncoder {
    deck1: bool,
    deck2: bool,
    deck3: bool,
    state: u8,
}

impl FilterEncoder {
    fn toggle(
        &mut self,
        note: u8,
        state: bool,
        out: &mut MidiOutputConnection,
        color_out: &mut MidiOutputConnection,
    ) -> Result<bool> {
        if let Some(i) = [
            DECK1_FILTER_TOGGLE_NOTE,
            DECK2_FILTER_TOGGLE_NOTE,
            DECK3_FILTER_TOGGLE_NOTE,
        ]
        .iter()
        .position(|&x| x == note)
        {
            // Off messages are captured, but ignored.
            if !state {
                return Ok(true);
            }

            let enabled = &mut [&mut self.deck1, &mut self.deck2, &mut self.deck3][i];
            **enabled = !**enabled;
            let enabled = **enabled;

            // Send filter encoder output to rekordbox.
            self.send(out)?;

            // Send color output back to device.
            if enabled {
                color_out.send(&[DEVICE_CHANNEL | NOTE_ON, note + 0x48, 127])?;
            } else {
                color_out.send(&[DEVICE_CHANNEL | NOTE_OFF, note + 0x48, 127])?;
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn adjust(&mut self, data: u8, out: &mut MidiOutputConnection) -> Result<()> {
        self.state = data;
        self.send(out)
    }

    fn send(&self, out: &mut MidiOutputConnection) -> Result<()> {
        for (enabled, cc) in [
            (self.deck1, DECK1_FILTER_CC),
            (self.deck2, DECK2_FILTER_CC),
            (self.deck3, DECK3_FILTER_CC),
        ] {
            if enabled {
                log_send(FILTER_ENCODER_CHANNEL, CONTROL_CHANGE, cc, self.state, out)?;
            } else {
                log_send(FILTER_ENCODER_CHANNEL, CONTROL_CHANGE, cc, 63, out)?;
            }
        }

        Ok(())
    }
}

impl Default for FilterEncoder {
    fn default() -> Self {
        Self {
            deck1: false,
            deck2: false,
            deck3: false,
            state: 63,
        }
    }
}

struct TempoEncoder {
    deck_index: usize,
    deck1_value: u8,
    deck2_value: u8,
    deck3_value: u8,
    prev_value: u8,
}

impl TempoEncoder {
    fn select_deck(&mut self, note: u8, color_out: &mut MidiOutputConnection) -> Result<bool> {
        let toggle_notes = [
            DECK1_TEMPO_TOGGLE_NOTE,
            DECK2_TEMPO_TOGGLE_NOTE,
            DECK3_TEMPO_TOGGLE_NOTE,
        ];

        if let Some(i) = toggle_notes.iter().position(|&x| x == note) {
            self.deck_index = i;

            // Toggle lights for other decks.
            for x in toggle_notes {
                let message = if x == note {
                    NOTE_ON
                } else {
                    NOTE_OFF
                };

                color_out.send(&[DEVICE_CHANNEL | message, x, 127])?;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn adjust(&mut self, data: u8, out: &mut MidiOutputConnection) -> Result<()> {
        let (cc, deck_value) = match self.deck_index {
            0 => (DECK1_TEMPO_CC, &mut self.deck1_value),
            1 => (DECK2_TEMPO_CC, &mut self.deck2_value),
            2 => (DECK3_TEMPO_CC, &mut self.deck3_value),
            _ => return Err("INTERNAL ERROR: Tempo deck index out of range".into()),
        };

        // Pickup algorithm: Don't do anything until the new value has passed the stored value.
        let prev_sign = (*deck_value).cmp(&self.prev_value);
        self.prev_value = data;
        if (*deck_value).cmp(&data) == prev_sign {
            return Ok(());
        }
        *deck_value = data;

        // Inverting the value of this, since I'm used to the Rekordbox controls where up =
        // slower, down = faster.
        log_send(TEMPO_ENCODER_CHANNEL, CONTROL_CHANGE, cc, 127 - data, out)?;

        Ok(())
    }
}

impl Default for TempoEncoder {
    fn default() -> Self {
        Self {
            deck_index: 0,
            deck1_value: 63,
            deck2_value: 63,
            deck3_value: 63,
            prev_value: 63,
        }
    }
}

struct State {
    headphones_mix: FakePotEncoder,
    headphones_volume: FakePotEncoder,
    master_volume: FakePotEncoder,
    filter_encoder: FilterEncoder,
    tempo_encoder: TempoEncoder,
}

impl State {
    fn new() -> Self {
        Self {
            headphones_mix: FakePotEncoder::default(),
            headphones_volume: FakePotEncoder::default(),
            master_volume: FakePotEncoder::default(),
            filter_encoder: FilterEncoder::default(),
            tempo_encoder: TempoEncoder::default(),
        }
    }

    fn transform(
        &mut self,
        message: &[u8],
        out: &mut MidiOutputConnection,
        color_out: &mut MidiOutputConnection,
    ) -> Result<()> {
        if message.len() == 3 {
            match message[0] & !DEVICE_CHANNEL {
                CONTROL_CHANGE => {
                    if self.handle_cc(message[1], message[2], out)? {
                        return Ok(());
                    }
                }
                state @ (NOTE_ON | NOTE_OFF) => {
                    let state = state == NOTE_ON;
                    if self
                        .filter_encoder
                        .toggle(message[1], state, out, color_out)?
                    {
                        return Ok(());
                    } else if self.tempo_encoder.select_deck(message[1], color_out)? {
                        return Ok(());
                    } else {
                        return handle_button(message[1], message[2], out);
                    }
                }
                _ => (),
            }
        }

        // If the handling above fails, just forward the message as-is.
        out.send(message)?;
        println!("PartySaver->RekordBox: {:?} (VERBATIM)", message);
        Ok(())
    }

    fn handle_cc(&mut self, cc: u8, data: u8, out: &mut MidiOutputConnection) -> Result<bool> {
        let pot_encoder = match cc {
            HEADPHONE_MIX_CC => &mut self.headphones_mix,
            HEADPHONE_VOLUME_CC => &mut self.headphones_volume,
            MASTER_VOLUME_CC => &mut self.master_volume,
            DECK1_LOOP_CC | DECK2_LOOP_CC | DECK3_LOOP_CC => {
                handle_fake_button(cc, data, out)?;
                return Ok(true);
            }
            FILTER_CC => {
                self.filter_encoder.adjust(data, out)?;
                return Ok(true);
            }
            TEMPO_CC => {
                self.tempo_encoder.adjust(data, out)?;
                return Ok(true);
            }
            _ => return Ok(false),
        };

        pot_encoder.add(data);
        pot_encoder.send(cc, out)?;
        Ok(true)
    }
}

fn main() -> Result<()> {
    // First, connect to an actual device.
    let device_in = MidiInput::new("PartySaver device in")?;
    let device_in_port = select_port(&device_in, "input")?;
    println!();
    let passthrough_device_out = MidiOutput::new("PartySaver device out")?;
    let device_out_port = select_port(&passthrough_device_out, "output")?;
    println!();

    println!("Opening connections");

    // Transform messages from the device to Rekordbox.
    let mut color_out =
        MidiOutput::new("PartySaver color out")?.connect(&device_out_port, "party-saver-color")?;
    let mut rb_out = MidiOutput::new("Rekordbox Out")?.create_virtual("PartySaver")?;
    let _conn_in = device_in.connect(
        &device_in_port,
        "party-saver",
        move |stamp, message, state| {
            println!(
                "Device->PartySaver {}: {:?} (len={})",
                stamp,
                message,
                message.len()
            );
            state
                .transform(message, &mut rb_out, &mut color_out)
                .unwrap_or_else(|e| {
                    println!("Failed to forward MIDI message to main thread: {}", e)
                });
        },
        State::new(),
    )?;

    // Forward all messages from rekordbox straight to the device.
    let mut passthrough_conn_out =
        passthrough_device_out.connect(&device_out_port, "party-saver")?;
    let _rb_in = MidiInput::new("Rekordbox In")?.create_virtual(
        "PartySaver",
        move |stamp, message, _| {
            passthrough_conn_out
                .send(message)
                .unwrap_or_else(|_| println!("Error when forwarding message ..."));
            println!(
                "Rekordbox->Device {}: {:?} (len = {})",
                stamp,
                message,
                message.len()
            );
        },
        (),
    )?;

    let mut input = String::new();
    stdin().read_line(&mut input)?; // wait for next enter key press

    Ok(())
}

fn select_port<T: MidiIO>(midi_io: &T, descr: &str) -> Result<T::Port> {
    println!("Available {} ports:", descr);
    let midi_ports = midi_io.ports();
    for (i, p) in midi_ports.iter().enumerate() {
        println!("{}: {}", i, midi_io.port_name(p)?);
    }
    print!("Please select {} port: ", descr);
    stdout().flush()?;
    let mut input = String::new();
    stdin().read_line(&mut input)?;
    let port = midi_ports
        .get(input.trim().parse::<usize>()?)
        .ok_or("Invalid port number")?;
    Ok(port.clone())
}
