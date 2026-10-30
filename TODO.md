# Planned Improvements

## Notifications

- [ ] Integrate native desktop notifications for important and critical events.
- [ ] Build a modern, consistent in-app toast notification component.
- [ ] Replace scattered, inconsistent toast messages with the shared component and unified placement and behavior.

## Streams Page

- [ ] Redesign the streams page with a modern look that matches the rest of the application.

## Settings Page

- [ ] Completely redesign settings to be more modern, user-friendly, and easy to navigate.

## Welcome Wizard

- [ ] Add a guided welcome wizard to help users configure the application.
- [ ] Guide users through API token setup, storing credentials in OS secure storage.
- [ ] Guide users through Debrid service configuration.
- [ ] Guide users through caching settings.

## Direct Torrent Playback

- [ ] Integrate a direct torrent engine using `librqbit` so users can stream without a Debrid service (currently an isolated POC only).
  - [ ] Investigate and improve the POC's approximately 8-second seek delay.
  - [ ] Connect torrent acquisition to local streaming playback, prioritizing requested pieces and waiting safely for missing data.
  - [ ] Add explicit opt-in controls explaining peer IP visibility, plus upload/seeding limits, cancellation, and storage management.
  - [ ] Validate real playback, seeking, failure recovery, and Windows/macOS/Linux behavior.

## External Subtitles

- [ ] Integrate subtitle services such as OpenSubtitles.
- [ ] Allow users to search for, download, and load external subtitles directly in the player.

## Cross-Platform Testing

- [ ] Test the application on macOS and Windows; hands-on testing so far has focused on Linux Wayland.
- [ ] Validate platform-specific behavior, including embedded playback, notifications, secure credential storage, and packaging.

## Packaging and Releases

- [ ] Configure a desktop installer for Windows.
- [ ] Plan MacOS installation and distribution, including feasibility and platform requirements for an MacOS build.
- [ ] Evaluate Linux AppImage packaging and standalone binary distribution.
- [ ] Configure a CD pipeline to build release artifacts and publish GitHub releases when a release tag is pushed.

## Embedded MPV Player

- [ ] Add seek-bar thumbnail previews.
- [x] Add default player keyboard/mouse controls and shared Settings/help reference ([scope and evidence](docs/design/player-shortcuts.md)).
- [ ] Qualify expanded player shortcuts on Windows/macOS and non-US keyboard layouts; add configurable bindings in a later phase.
- [ ] Polish the player chrome to match the application's design system.
- [ ] Add and validate HDR playback support, including correct color output and tone mapping on supported displays and platforms.
- [ ] Improve and validate 4K and other high-resolution, high-quality playback, including hardware decoding and performance on supported devices.

## Library and Watch History Portability

- [ ] Add import and export for the user's library and watch history.
- [ ] Explore optional cloud or free-service integration for exporting and importing data across multiple devices.
- [ ] Define safe merge and conflict handling when importing data from another device, and exclude credentials from exports.

## Mobile Platforms and Responsive Layouts

- [ ] Investigate expanding platform support to Android and iOS, including UI framework and embedded playback feasibility.
- [ ] Adapt layouts and navigation for smaller screens and touch interaction.
- [ ] Validate mobile usability and playback on real devices before claiming platform support.

## Scrolling

- [ ] Introduce smooth scrolling throughout the application to improve the user experience.
