# OpenWA

OpenWA is a work-in-progress open source re-implementation of the 1999 PC game Worms Armageddon™ (WA), written in Rust. The goal is to produce a version of the game that supports multiple platforms, modding and opt-in enhancements, while remaining highly accurate to the original game.

**Contributing to and using OpenWA requires a legally acquired copy of Worms Armageddon™**, which is available on [Steam](https://store.steampowered.com/app/217200/Worms_Armageddon/) and [GOG](https://www.gog.com/en/game/worms_armageddon). This project does not and will not include any of the original game's assets.

OpenWA is licensed under the GPLv3 license. See [LICENSE.md](LICENSE.md) for details.

## Project status (2026-04)

At the time of writing, OpenWA is still in very early stages of development. While the project has seen quite a lot of development relative to its age, OpenWA does not yet offer anything of substance to end users. There is no support for platforms other than Windows for the time being, and the current version is essentially a mod for WA's Steam release (3.8.1).

## Quick start

- Ensure you own a copy of Worms Armageddon™ on Steam, you've installed it and run it at least once.
- Clone the repository.
- Install the 32-bit MSVC toolchain (`i686-pc-windows-msvc`) with `rustup`, if you haven't already.
- Build & run the project by using `start.ps1` or `start-debug.ps1` (for a build with the debug UI).
  - OpenWA should be able to find your WA installation automatically using the registry, but if it doesn't, follow the instructions in the error message to provide it with the correct path.
- You can also run tests with various commands:
  - `cargo test` for standard unit, integration and snapshot tests.
  - `run-tests.ps1` for headless replay tests. You can pass `-j` to specify the number of concurrent tests to run. Running tests with high concurrency can cause flakiness at times, due to currently unknown issues with process isolation.
  - `replay-test.ps1` to run one or more replay tests with graphics and audio (but at high speed). The name of the test(s) is relative to the `/testdata/replays` directory.

## Scope, goals and non-goals

The main goal is to create an accurate _re-implementation_ of the original WA, not a game inspired by it, which is the primary difference from existing open source Worms-likes such as Hedgewars and Wormux/Warmux. OpenWA is largely based on decompilation and behavioral analysis of the original WA.exe, and in theory it should be able to perfectly replicate the original game's gameplay and graphics.

### Goals

- Tick accurate re-implementation of WA, with all single-player and multiplayer modes and features working like in the original game.
- Support for playing back replays recorded from any unmodified version of the game, without desyncs or visual glitches.
- Graphics and audio that are indistinguishable from the original game &mdash; unless configured otherwise, of course.
- Long term: support for all common desktop platforms, including Windows, Linux / SteamOS and macOS.
  - This also requires support for multiple CPU architectures, such as x86-64 and ARM64.
- Long term: memory safe, idiomatic Rust code.
  - We are a ways off from this at the moment; the vast majority of the codebase is currently `unsafe`. We can't really make use of Rust-native structs (non-`repr(C)`) until we've ported most code in each subsystem, or at least wrapped all field accesses by WA.exe into safe getter and setter functions.
  - This will require extensive rearchitecting and refactoring, as WA was written in C++ and uses quite a lot of (multiple) inheritance, as well as mutable global state.
- Long term: UI scaling and high DPI support.
- Long term: accessibility features, such as remappable controls and colorblind modes.
- Long term: increased engine limits, such as support for more weapons, larger maps, more teams / worms, etc.
- Long term: a modern GPU renderer with significantly reduced CPU usage.
  - It's a 1999 game, but that doesn't mean it can't be optimized to run better on modern hardware. And writing renderers is fun!
  - Caveat: the GPU renderer probably won't be _exactly_ pixel perfect to the original.

### To be determined

- WA's menu system (AKA the frontend) is an iconic part of the game, and quite important to the overall feel. However, it was built with [MFC](https://en.wikipedia.org/wiki/Microsoft_Foundation_Class_Library), which is _very_ tightly coupled to Windows.
  - We will likely have to start-off with a basic frontend implemented with a Rust-native UI framework, for example [egui](https://github.com/emilk/egui).
  - Eventually, we should aim to replicate the original frontend's look and feel as closely as possible, possibly using a custom UI framework.
- It remains to be seen if OpenWA can be backwards compatible with the original WA. We want to be able to play back old replays and use same schemes and levels, but we probably want to introduce our own formats at some point to increase limits and to add features. So it is likely that OpenWA can read data produced by WA.exe, but not the other way around.

### Non-goals / out of scope

- WormKit will not be supported. WormKit isn't really a modding API, but rather just a built-in DLL loader. All WormKit mods are essentially DLLs that hook & modify the game in various ways (just like OpenWA itself at the moment), and are tightly coupled to WA.exe's memory layout and other internal implementation details.
  - Since OpenWA is open source, it can be modded much more easily than the original WA.
- Multiplayer with unmodded WA is not something we aim to support. Any incompatibilities between OpenWA and the original will cause desyncs in multiplayer, bothering users who just want to play online. The open nature of OpenWA combined with the game's architecture (deterministic lockstep) means some forms of cheating are impossible to prevent (though also some cheats are just impossible by design), and at the moment we are not capable of implementing any meaningful anti-cheat measures.
- Supporting legacy platforms (such as old versions of Windows) is not planned. The code is open so you can try to port it to whatever platform you want, but OpenWA's main focus is on modern desktop platforms.
- OpenWA will probably not support all of the graphics backends that WA 3.8.1 does. Software rendering is likely to be supported indefinitely, but there's no good reason to write new DirectDraw or D3D7 code in `$current_year` (other than for fun and nostalgia, of course). OpenWA will likely support a few modern graphics APIs via `wgpu` or SDL3.

## WA fundamentals

- WA.exe is a 32-bit x86 Windows application. The current Steam version (3.8.1) was compiled with MSVC 2005.
- WA was written in C++ but it uses C++ features rather sparingly, as was common with games of that era. Think classes (single and multiple inheritance), exceptions in a few places, and very few signs of templates and / or the STL.
- WA relies heavily on Windows APIs and libraries, such as MFC, DirectDraw and DirectSound. Game logic is mostly decoupled from platform APIs.
- While WA's Steam version supports multiple graphics APIs (DirectDraw, D3D and OpenGL), in practice the game is 99.99% based on software rendering. Graphics APIs are used to present the final framebuffer to the screen, and with some backends a simple pixel / fragment shader is used to apply palette effects.
- WA uses deterministic lockstep simulation for game logic, much like most RTS games.
  - Replays and multiplayer work by recording and replaying player inputs, and the game state is never serialized or transmitted over the network.
  - Game logic must be 100% deterministic despite the presence of pseudo-random number generation, which is achieved by synchronizing the RNG state at the start of a match / replay, and by making sure the RNG is sampled in the same order on all machines.
- To make determinism easier to achieve (and probably because of the game's Amiga/DOS heritage), WA almost never uses floating point math. Instead, it uses 16.16 signed fixed-point arithmetic (`struct Fixed` in OpenWA) for all game logic and most graphics code.
- The game has a fairly standard entity / game object hierarchy. Entities are called "tasks" in OpenWA, a naming convention I inherited from WormKit / wkJellyWorm (and might change in the future). Tasks communicate using message passing; a large share of game logic is implemented in each task class's `handle_message` method.

## Technical approach

The re-implementation approach is very similar to the one used by OpenRCT2 in its early stages:

- A DLL (`openwa.dll`) is injected to WA.exe using a custom launcher.
  - The project started as a WormKit DLL, but I had to replace it with a custom injector as patching the game while the main thread was running was unreliable (and fundamentally incapable of patching everything).
- The DLL replaces key functions in the original game with Rust implementations, using [MinHook](https://github.com/tsudakageyu/minhook) and vtable patching to redirect execution to Rust code.
  - At this stage, most Rust code is `unsafe`, probably some of it even unsound, and looks more like C than Rust. The current focus is to match the original behavior as closely as necessary, even at the cost of safety and code beauty.
- As more and more functions are re-implemented in Rust, hooks and FFI calls can be replaced with direct Rust-to-Rust calls.
  - At some point we can start using structs allocated by the standard Rust allocator, which enables the use of Rust-native data structures and safe code.
  - WA.exe functions that are no longer called are trapped (meaning they cause a crash on invocation) to guarantee they are never accidentally used again.
- At some point, the entire game has been re-implemented in Rust, and the original WA.exe can be replaced with a new completely independent executable that uses the Rust code directly without any hooks or FFI.
  - This eventually enables support for platforms other than 32-bit x86 Windows.

## Testing

To ensure the correctness and stability of the re-implementation, testing is vitally important. OpenWA has several types of tests, but by far the most important ones are _replay tests_. It turns out WA's replay system can be quite easily retrofitted into a very powerful testing tool.

As previously stated, a replay is a recording of player inputs. This means that from a gameplay logic perspective, playing back a replay runs almost all the same code paths as playing a match normally. The game records checksums of the game state periodically during replay recording, and the checksums are validated during playback to detect desyncs.

WA can also run replays headlessly (i.e., without graphics or audio). This feature is primarily accessed via the `/getlog` startup parameter, which runs the provided replay file and produces a timestamped log of all major game events, such as weapons being fired, worms taking damage, etc.

Combining these features enables recording gameplay from the original unmodified WA.exe, and then using the recorded replay files (+ their associated logs) as test cases for OpenWA. With the help of some custom tooling and test-specific API hooks we can run several tests concurrently at unrestricted speed, simulating hours of gameplay in a matter of seconds.

If all replays pass without crashing, don't produce any desync warnings, and generate logs byte-for-byte identical to the original, we can be fairly confident that the re-implementation is correct, as long as the functionality being tested is covered by the replays. Tests can be run both headlessly and headfully (with graphics and audio), but at the moment most graphics and audio related changes still require human validation.

This is what enables OpenWA to be highly accurate to the original game.

There are of course other types of tests as well. We use snapshot tests for testing some functions that operate on relatively large amounts of data that have to be perfectly accurate &mdash; namely line drawing and sprite blitting functions. And there are of course unit and integration tests, as you'd expect.

## Q&A

### Is this AI slop? Almost all of the commits have Claude as a co-author!

What constitutes "AI slop" depends on one's definition of it.

Yes, this codebase has been largely developed with the help of Claude Code, using [Ghidra MCP](https://github.com/bethington/ghidra-mcp) to enable Claude to perform the vast majority of the reverse engineering and re-implementation work. However, I wouldn't describe it as a _vibe coding_ project, as I've been very actively involved in the development process, I have made all the architectural decisions, and I do understand the codebase and the original game's architecture quite well at this point.

This project has been a personal experiment to investigate how well state of the art AI tools can do reverse engineering of a fairly complex game, and the answer so far is: surprisingly well. Language models are fundamentally pattern recognition engines, and that is largely what reverse engineering is all about. I like reverse engineering and game development (I've written about both topics in [my blog](https://blog.paavo.me/)), but to be honest I don't particularly enjoy reading or writing assembly, or debugging issues with call conventions until two in the morning.

For better or worse, while Claude has been quite good at the function level RE work, it still requires constant human supervision and advice to keep it from going completely insane. Claude can be very lazy in this sort of work; it often produces code that uses untyped raw pointers, pointer arithmetic and magic constants, even when there are perfectly fine structs and enums defined for the data being manipulated. This requires either manual refactoring or additional prompting to get it to produce the wanted results.

I have been in control of every architectural decision, such as which dependencies to use, how to structure the codebase, and what kind of debug and testing tools would be useful for the project. That doesn't mean those decisions are _good_, but you can mostly blame me for them.

Even when a commit has involved a significant amount of good old fashioned human work, I've often used Claude to commit the changes for me, just to keep the flow going. Personally I don't think AI agents are particularly good at writing useful commit messages (as they rarely know the human context behind the decisions involved), but this early in the project I frankly don't care that much about the commit history looking like a mess.

Personally, I let the results speak for themselves. The code can be a bit _sloppy_ at times; while I've been doing a lot of refactoring and cleanup, the main focus at this point is to get something working so that we can move on to the independent executable stage as soon as reasonably possible. I do believe my constant supervision and rigorous approach to testing keeps it from being outright _slop_.

### What is the point of using Rust, if 90% of the code is `unsafe` and looks like C?

Simple answer: I don't like coding in C or C++, and I do enjoy writing Rust. I've used Rust as my primary hobby programming language for a bit under 10 years at this point, so it's a natural choice for me.

But it is true that this is one of the weirdest Rust codebases I've ever worked on or even seen. OpenWA currently exists as a DLL injected to a game written in late 90s C++, and that means it's not going to be beautiful no matter the approach. Almost every struct has to be `repr(C)` with fields at exact offsets in order to maintain compatibility with WA.exe, and calls between Rust and C++ sometimes require a lot of FFI glue. I have tried to make it more bearable with neat tricks like typed vtables with macro-generated method wrappers, but it's still going to be a bit of a mess for a while.

## Thanks and sources

- [WormKit](https://github.com/CyberShadow/WormKit)
- [wkJellyWorm](https://github.com/nizikawa-worms/wkJellyWorm)
- [Worms Knowledge Base](https://worms2d.info/Main_Page)

## Disclaimer

"Worms Armageddon" is a registered trademark of Team17 Software Limited in the EU and other countries. OpenWA is an independent and strictly non-commercial fan project, and is not affiliated with or endorsed by Team17 Software Limited. OpenWA does not include any of the original game's assets, and requires a legally acquired copy of "Worms Armageddon" to use.
