# spotatui on an Android TV box (community setup)

> **Unofficial.** This is not a supported way to install spotatui. It shows that the terminal app runs fine on an unusual device: the Linux build runs inside a Debian container in Termux, draws fullscreen on the TV, and plays through the TV's HDMI audio. If something breaks here, ask on [Discord](https://spotatui.com/discord) or in [Discussions](https://github.com/LargeModGames/spotatui/discussions), not in an issue.

It was set up and tested on an NVIDIA SHIELD TV Pro (2019, Android 11, no root). The steps should work on other 64-bit Android TV boxes, but only the SHIELD was tried.

## What you get

- The normal spotatui terminal UI fullscreen on the TV, cover art in full resolution, lyrics and the live visualizer.
- Audio through the box's own output (HDMI), with native streaming or Soloist.
- One tile on the home screen that opens spotatui, and quitting with `q` closes everything.
- Self-update works, because it is the regular Linux release.

## What you need

- **A 64-bit box.** `adb shell getprop ro.product.cpu.abilist` must list `arm64-v8a`. spotatui has no 32-bit ARM release, and some boxes run only 32-bit apps even with a 64-bit chip (reportedly the 2019 SHIELD TV "tube"), so check before you start.
- **A computer with `adb`** on the same network, and **network debugging** on the box (Developer options).
- **A keyboard, or the Spotify app on your phone.** The remote's D-pad works as arrow keys, Select as Enter and Back as Esc, but it has no letter keys. With [Native Streaming](native-streaming.md) on, the phone app controls spotatui's Connect device.
- **A release with the visualizer fallback** ([#959](https://github.com/LargeModGames/spotatui/pull/959)) for the live visualizer. An older release starts and plays, but the visualizer gets no audio, because there is no PipeWire server on Android.

## 1. Install Termux

Install [Termux](https://github.com/termux/termux-app) from its official GitHub page, the `arm64-v8a` APK:

```sh
adb connect <box-ip>:5555        # accept the prompt on the TV
adb install termux-app_*_arm64-v8a.apk
```

If Play Protect refuses the first install, run `adb install` again; the TV may also ask you to confirm.

For cover art in full resolution, Termux needs Sixel support, which was merged after v0.118.3 and is only in the [master build](https://github.com/termux/termux-app/actions/workflows/debug_build.yml) for now (the `apt-android-7-github-debug_arm64-v8a` artifact). It has the same signing key as the GitHub release, so it installs over it with `adb install -r -d` and keeps your data. Without it, spotatui hides the cover art.

Open Termux once on the TV, then install the packages:

```sh
pkg upgrade
pkg install openssh tmux pulseaudio proot-distro
proot-distro install debian
```

## 2. Install spotatui in Debian

```sh
proot-distro login debian
apt update
apt install --no-install-recommends ca-certificates curl libasound2t64 libasound2-plugins libssl3t64 libpipewire-0.3-0t64
curl -fsSL https://github.com/LargeModGames/spotatui/releases/latest/download/spotatui-linux-aarch64.tar.gz | tar -xz -C /usr/local/bin
exit
```

The release links the PipeWire library, so `libpipewire-0.3-0t64` must be installed even though no PipeWire server runs.

Until a release with the visualizer fallback ([#959](https://github.com/LargeModGames/spotatui/pull/959)) is out, `latest` installs one without it: everything works except the live visualizer. Once you are on a newer release, self-update keeps you there.

## 3. Route the audio

spotatui plays through ALSA. In Debian, send ALSA to Termux's PulseAudio, which plays through Android. Put this in Debian's `/etc/asound.conf`:

```
pcm.!default { type pulse }
ctl.!default { type pulse }
pcm.pulse_monitor {
  type pulse
  device "OpenSL_ES_sink.monitor"
  hint { show on description "PulseAudio monitor of the HDMI output" }
}
```

`pulse_monitor` is what the visualizer records. It only opens a device with "monitor" or "loopback" in its name, never the default input, so this entry is required for the visualizer.

## 4. The launch script

Create the folder for the scripts first:

```sh
mkdir -p ~/bin ~/.termux
```

Save this as `~/bin/spotatui-tv` in Termux and make it executable (`chmod +x`):

```sh
#!/data/data/com.termux/files/usr/bin/bash
# Starts sshd and PulseAudio if needed, then spotatui inside Debian.
pgrep -x sshd >/dev/null || sshd
pulseaudio --check 2>/dev/null || pulseaudio --start --exit-idle-time=-1 \
  --load="module-native-protocol-tcp auth-ip-acl=127.0.0.1 auth-anonymous=1" 2>/dev/null
pactl set-sink-volume @DEFAULT_SINK@ 100%
exec proot-distro login debian -- env PULSE_SERVER=tcp:127.0.0.1:4713 \
  TERM=xterm-256color COLORTERM=truecolor LANG=C.UTF-8 /usr/local/bin/spotatui "$@"
```

`TERM=xterm-256color` matters: inside tmux, spotatui would otherwise send the cover art past tmux, which garbles the screen. With it, tmux draws the image itself. PulseAudio stays at 100%; set the loudness on the TV.

And `~/bin/spotatui-tv-loop`, which quits everything when you quit spotatui. Termux on Android TV never closes its last window by itself, so the loop stops Termux the way its own Exit button does:

```sh
#!/data/data/com.termux/files/usr/bin/bash
# Runs spotatui; quitting closes Termux, a crash restarts it after 5 s.
while :; do
  "$HOME/bin/spotatui-tv" && break
  read -r -s -n 1 -t 5 -p "spotatui crashed. Restarting in 5 s, press any key for a shell. " && exec bash -l
  echo
done
pulseaudio --kill 2>/dev/null
pkill -x sshd
termux-wake-unlock
am startservice -n com.termux/.app.TermuxService -a com.termux.service_stop >/dev/null 2>&1
tmux kill-server
```

## 5. Start it with Termux

Add this to `~/.bashrc` in Termux. When Termux opens on the TV, it runs spotatui in a tmux session called `tv`; an SSH login gets a normal shell:

```sh
if [[ $- == *i* ]] && [ -z "$SSH_CONNECTION" ] && [ -z "$TMUX" ]; then
  termux-wake-lock
  tmux new-session -A -s tv "$HOME/bin/spotatui-tv-loop"
  exit 0
fi
```

`~/.tmux.conf`:

```
set -g default-terminal "tmux-256color"
set -as terminal-features ",xterm-256color:RGB:sixel"
set -g status off
set -sg escape-time 10
```

And add to `~/.termux/termux.properties` for a fullscreen terminal without the extra-keys row, with Back as Esc:

```
fullscreen = true
use-fullscreen-workaround = true
extra-keys = []
hide-soft-keyboard-on-startup = true
back-key = escape
```

Restart Termux. To get a tile, add Termux to the favorite apps on the home screen.

## 6. First launch

spotatui asks about the song counter and the source. When you pick Spotify, the status bar shows a pairing link and code, `https://spotify.com/pair?code=...`. Android may also show an "Open with" dialog for the link: press Back and open the link on your phone instead. Log in there and confirm the code.

## Daily use

- **Open:** the Termux tile. If spotatui already runs, you get it back as it was; pressing Home leaves the music playing.
- **Quit:** `q` and Enter. spotatui, PulseAudio, sshd and Termux all stop.
- **Control from a computer:** set up an SSH key in `~/.ssh/authorized_keys` (sshd listens on port 8022), then `ssh -t -p 8022 <box-ip> tmux attach -t tv -f ignore-size`. Your keyboard types into the TV session and the TV keeps its size. Add `,read-only` to only watch. `Ctrl+B D` leaves.
- **Updates:** spotatui updates itself like any Linux install.

## Closing from the recent apps screen

Android never tells Termux that it was closed from the recent apps screen, because its background service keeps running. If you want that to quit spotatui too, Termux can watch its own task with `adb` (`pkg install android-tools`, then accept the prompt on the TV once for `adb connect 127.0.0.1:5555`). Save this as `~/bin/spotatui-tv-watch`, start it with `"$HOME/bin/spotatui-tv-watch" &` at the top of the loop script, and add `pkill -f spotatui-tv-watch` before `pulseaudio --kill`. It quits spotatui with `q` and Enter, so the loop closes everything as usual:

```sh
#!/data/data/com.termux/files/usr/bin/bash
# Quits spotatui once Termux is gone from the recent apps.
while :; do
  adb connect 127.0.0.1:5555 >/dev/null 2>&1
  adb -s 127.0.0.1:5555 shell 'while dumpsys activity recents | grep -q "A=[0-9]*:com.termux "; do sleep 3; done' >/dev/null 2>&1 && break
  sleep 5
done
tmux send-keys -t tv q; sleep 1; tmux send-keys -t tv Enter
```

This gives Termux the same rights as `adb` on the box, and needs network debugging to stay on, which lets any authorized device on your network control the box. Use it only on a network you trust, and skip it if `q` is enough for you. To undo it, turn off network debugging and revoke the USB debugging authorizations in Developer options.

## Troubleshooting

- **No sound:** check that PulseAudio runs (`pulseaudio --check`) and that `pactl list short sink-inputs` shows a stream while spotatui plays.
- **The visualizer says it has no audio:** the `pulse_monitor` entry is missing from `/etc/asound.conf`, or the release is older than the visualizer fallback.
- **No cover art:** Termux has no Sixel support yet (see step 1). A garbled screen instead means `TERM` is not `xterm-256color` in the launch script.
- **spotatui does not start:** `libpipewire-0.3-0t64` is missing in Debian.

## Remove it

Uninstalling Termux removes Debian, spotatui, the config and the Spotify login. If you set up the recent-apps watcher, also revoke the USB debugging authorizations in Developer options.
