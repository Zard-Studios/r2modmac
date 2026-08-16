# Stopping a launch

Pressing Play does not start a game directly. r2modmac asks **Steam** to start it, and
Steam decides when that happens. If Steam is closed, it has to open and sign in first,
which can take a couple of minutes on a cold start.

While that is going on, the Play button turns into the same red **stop** button used for a
running game. Pressing it stops the launch.

## What stopping does

| Situation when you press stop | What happens |
| --- | --- |
| Steam was **closed** and r2modmac opened it | r2modmac stops waiting **and asks Steam to quit** |
| Steam was **already open** before you pressed Play | r2modmac stops waiting; your Steam stays open |
| The game has already started | The button is the normal Stop Game button, and stops the game |

The rule behind the table: **r2modmac only closes the Steam it opened itself.** A Steam you
already had running may be downloading something or signed into something you care about,
so it is never closed behind your back.

Steam is asked to quit the same way its own menu does — never force-killed, because
killing Steam mid-boot is what used to leave it crashing on the next start. It takes
Steam a few seconds to answer (measured at 8 seconds from a cold start on macOS), and
that happens in the background: the button comes back immediately.

## Why the game can still appear after you press stop

By the time the button can be pressed, the request has already left r2modmac. If Steam is
far enough along, it may still start the game before it acts on the shutdown — and if Steam
was already open, nothing is sent to it at all, so the game will normally start.

This is a limit of what Steam offers, not a bug: a `steam://run` request cannot be recalled.
When it happens, the button simply becomes the Stop Game button, which does stop the game.

## Timing

Cancelling stops the *waiting* instantly. Closing Steam takes longer, because Steam has to
finish coming up before it can be told to go away. Measured on a cold start:

| Platform | Steam closed after |
| --- | --- |
| macOS native | ~8 seconds (~15 when stop is pressed 1 second after Play) |
| CrossOver (Wine) | ~9.5 seconds, from a stop pressed 1 second after Play |

The button comes back immediately either way — the closing happens in the background.

## How Steam is asked to close

On **macOS** two routes are alternated every few seconds until Steam goes away, for up to
30 seconds:

| Route | Behaviour |
| --- | --- |
| `osascript -e 'tell application id "com.valvesoftware.steam" to quit'` | Addresses Steam by bundle id, so it finds the copy under `~/Library/Application Support/Steam/Steam.AppBundle` that actually runs — not just an install in `/Applications` |
| `open steam://exit` | Valve's shutdown URL. A **booting** Steam ignores it outright; a booted one answers in a few seconds |

Neither route works on a Steam that is still early in its boot, which is exactly when a
launch gets cancelled — hence the alternating retries rather than one attempt each. Success
is decided by looking for Steam's processes, never by an exit code: the AppleScript quit
reports error -128 ("cancelled by the user") while quitting Steam perfectly well.

Under **Wine, CrossOver, Sikarugir and on Windows** it is `steam.exe -shutdown`, Steam's own
shutdown switch, sent through whichever runner started the client — the Wineskin wrapper, the
CrossOver or Wine runner, or Windows itself. Nothing here knows *which* launcher it is
talking to: the route is simply the one that started this Steam, so a launcher that can start
a game can stop it.

Two measured details that are easy to get wrong:

- **Wait for Steam before asking.** The user cancels about a second after pressing Play,
  when Steam does not exist yet. A single request at that moment lands on nothing and Steam
  boots on to start the game — which is what made this look broken. The watch waits for
  Steam to appear, then asks, and keeps asking.
- **Do not count your own requests.** `steam.exe -shutdown` is itself a process carrying the
  path of `steam.exe`, and it does not exit after delivering the request. A watch that counts
  it never sees Steam go away. Worse, `sysinfo` does not report the arguments of Wine-hosted
  processes — our shutdown command appears there only as a copy of `steam.exe` in Wine's temp
  directory — so the exclusion reads command lines from `pgrep -fl` instead.
- **Ask which program a process *is*, not which one it mentions.** Steam's UI helper runs as
  `steamwebhelper.exe … -steampath=C:\…\steam.exe …` and outlives the client by a long way;
  the Wine launcher runs as `winewrapper.exe --run -- /…/steam.exe`. Counting either one means
  concluding Steam is still up long after it has closed. The program is the first `.exe` on
  the line — not split on whitespace, because `C:\Program Files (x86)\Steam\steam.exe` has
  spaces in it, and not split on the first argument, because launchers append environment
  assignments with no leading dash.

### What is never done

- **`taskkill /F`** — force termination. Wine's `taskkill` sends `WM_CLOSE` without `/F` and
  terminates with it ([wine source](https://github.com/wine-mirror/wine/blob/master/programs/taskkill/taskkill.c)).
- **`wineserver -k`** — kills every Wine process in the prefix, which would take the game and
  anything else in that bottle with it ([WineHQ forums](https://forum.winehq.org/viewtopic.php?t=6330)).
- **Killing the native macOS Steam.** Killing Steam mid-boot is what left it crashing on the
  next start, and forcing a Steam client down is a known way to corrupt its `.vdf`/`.acf`
  files, after which games go missing or need verifying
  ([Proton issue #114](https://github.com/ValveSoftware/Proton/issues/114)).

On macOS, a Steam that refuses to close is therefore left running, and the log says so. Under
Wine the ladder above applies instead.

### The ladder used under Wine

Asking politely is not enough on its own. A Steam client that is still booting answers
`-shutdown` with nothing at all — measured still deaf three minutes in, on a bottle on an
external disk — while booting straight into the `-applaunch` it was handed. Waiting politely
means watching the game start.

So the shutdown follows the ladder a Wine session manager uses
([winetop](https://github.com/akovari/winetop)), stopping short of its last rung:

| Rung | What it is | When |
| --- | --- | --- |
| 1 | `steam.exe -shutdown` | first three rounds, ~9 seconds |
| 2 | `SIGTERM` to the client's host process | if Steam is still up |
| 3 | `SIGKILL` to the same process | if it survived `SIGTERM` |
| — | ~~`wineserver -k`~~ | **never** |

A Wine-hosted Windows program is a real process on this side, so signalling that process is
what actually ends it: measured, the client was gone one second after `SIGTERM`, its wrapper
with it. Nothing about the launcher is involved — the process is found by which executable it
*runs*, not by any name the launcher gives it, so the same code reaches a client started by
CrossOver, Sikarugir, Whisky or plain Wine.

`wineserver -k` is refused because it ends every process in the prefix
([WineHQ](https://forum.winehq.org/viewtopic.php?t=6330)) and would take a game running in the
same bottle with it. `taskkill /F` is the same idea from the inside
([wine source](https://github.com/wine-mirror/wine/blob/master/programs/taskkill/taskkill.c)).

Escalating early is deliberate, and it is also when it costs least: forcing a Steam client
down can corrupt its `.vdf`/`.acf` files, after which games go missing or need verifying
([Proton issue #114](https://github.com/ValveSoftware/Proton/issues/114)) — but a client a few
seconds into its boot has barely written anything, and it is exactly then that stopping it is
the difference between a cancelled launch and a game that starts anyway.

**macOS native does not use this ladder.** The AppleScript quit works there, so nothing is
signalled: a Steam that refuses is left running.

This behaviour belongs to macOS and to Steam, not to r2modmac, and it has already changed
once during development. There is a test that exercises the real thing against the real
Steam, ignored by default because it starts and closes it:

```
cd src-tauri && cargo test --lib shuts_a_booting_steam_down_for_real -- --ignored --nocapture
```

The Wine side has the same test, and since no machine has every launcher installed, it is
pointed at whichever one you have — CrossOver, Sikarugir, Whisky, plain Wine:

```
R2MODMAC_TEST_WINE_RUNNER="/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine" \
R2MODMAC_TEST_WINE_PREFIX="/path/to/bottle-or-prefix" \
R2MODMAC_TEST_STEAM_EXE="/path/to/drive_c/Program Files (x86)/Steam/steam.exe" \
cargo test --lib shuts_a_wine_steam_down_for_real -- --ignored --nocapture
```

It skips itself when those are unset, and needs a Steam that is signed in (see above).

## Platforms

Stopping works on every launch path, because they all wait the same way:

- **macOS native games** — launched directly, or through Steam
- **Windows games on macOS** — CrossOver, Sikarugir/Wineskin, plain Wine
- **Windows games on Linux** — Wine, through the same runner code as macOS
- **Windows** — Steam natively

The wait polls for the cancellation every 250 ms, so pressing stop takes effect well within
a second on all of them, whatever deadline the launch was working to.

## For developers

The pieces, all under `src-tauri/src/commands/game_commands/`:

- `launch_cancel.rs` — the flag, the `cancel_game_launch` command, the "only close the Steam
  we opened" rule, and the macOS shutdown. `LAUNCH_CANCELLED_MESSAGE` is the sentence a
  cancelled launch reports; the frontend matches on it (`src/utils/launchIssue.ts`) to keep
  an error dialog off the screen for something the user asked for. **The two sides agree by
  string only** — tests on both sides pin it.
- `process.rs` — `StartWait` (`Started` / `TimedOut` / `Cancelled`) and the core wait loop,
  which takes "is it running?" and "should I stop?" as closures so it is testable without a
  real game or a real Steam.
- `steam_state.rs` — the Steam-watching wait used by the Windows and Wine paths, with the
  same `Cancelled` outcome.
- `macos/steam.rs` — the macOS Steam observation loop, plus the shutdown on cancel.
- `windows/mod.rs` — `WindowsSteamShutdown`, which records *how* this Steam was started
  (Wineskin wrapper, Wine/CrossOver runner, or natively) so the same route can stop it.

A cancellation is cleared at the start of every launch, so pressing Play after stopping one
launch never aborts the next one instantly.

Two things deliberately left alone:

- **Nothing is killed.** Cancelling ends the waiting and asks Steam to quit; it never sends
  a signal to Steam or to a game.
- **The OWML patcher is not cancellable.** It rewrites `Assembly-CSharp.dll` and finishes in
  seconds; interrupting it could leave a half-patched DLL behind.

Reported as [issue #36](https://github.com/Zard-Studios/r2modmac/issues/36).
