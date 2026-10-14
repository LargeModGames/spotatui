# Friends

The **Friends** screen shows other spotatui users you follow: who is online and what they listen to now. It is in the terminal app only, and it needs no Spotify session.

Friends runs on the spotatui.com service, with an account separate from your Spotify login.

## Setup

1. Create an account at [spotatui.com](https://spotatui.com).
2. Copy your **sync token** from the spotatui.com dashboard.
3. Paste it into Settings > Behavior > **Sync Token**, or set `behavior.sync_token` in `config.yml`.

Until a sync token is set, the Friends screen asks for an account instead of showing a list.

## Opening the screen

Open it from the **Library** menu > **Friends**. It has no global key. Your own friend code is at the top, your friends below, with the tabs `All (n)` and `Online (n)`. The list loads when you open the screen and refreshes every 30 seconds while the screen is open.

## Keys

| Key | Action |
| --- | --- |
| `↑` / `k`, `↓` / `j` | Move the selection |
| `H` / `L` | Jump to the top / bottom |
| `c` | Copy your friend code |
| `a` | Open the **Add Friend** dialog |
| `u` | Unfollow the selected friend (no confirmation) |
| `Tab` | Switch between All and Online |
| `Esc` | Clear the filter, or go back |

To filter the list by name, start typing. A first letter that is also a global key (for example `n`, `p`, `d` or `/`) runs that key instead; once the filter has text, every letter goes into it, and only the arrow keys and `Ctrl+N` / `Ctrl+P` move the selection.

### Adding a friend

Press `a`. The dialog has two tabs; `Tab` switches between them:

- **By Friend Code:** type the code and press `Enter`.
- **Search by Name:** type at least two characters of a name or code. Results you already follow are tagged. Move with `↑` / `↓` (or `Ctrl+N` / `Ctrl+P`) and press `Enter` to follow.

`Esc` closes the dialog.

## What it shows

For each friend: online or offline, the display name, and the track and artist they play now (or idle or offline). The data comes from `https://spotatui.com/api/`, sent with your sync token.
