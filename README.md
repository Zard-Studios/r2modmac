# r2modmac

<p align="center">
  <img alt="1024x1024@1x" src="https://github.com/user-attachments/assets/0069e8c1-79be-4edc-a235-7a351e0d5a49" />
  <br><br>

  <a href="https://github.com/Zard-Studios/r2modmac/releases">
    <img src="https://img.shields.io/github/v/release/Zard-Studios/r2modmac" alt="GitHub release">
  </a>

  <a href="https://github.com/Zard-Studios/r2modmac/releases">
    <img src="https://img.shields.io/github/downloads/Zard-Studios/r2modmac/total?label=Downloads" alt="Total Downloads">
  </a>

  <a href="https://opensource.org/licenses/MIT">
    <img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT">
  </a>

  <a href="https://ko-fi.com/zardstudios">
    <img src="https://img.shields.io/badge/Ko--fi-Support%20me-ff5f5f?logo=ko-fi&logoColor=white" alt="Ko-fi">
  </a>

  <a href="https://paypal.me/FedexPower">
    <img src="https://img.shields.io/badge/PayPal-Donate-00457C?logo=paypal&logoColor=white" alt="PayPal">
  </a>
</p>

## Description

r2modmac is a native mod manager for macOS/Windows that allows you to easily manage mods for Thunderstore supported games and Outer Wilds (via OWML). Designed with a modern and intuitive interface, it offers a smooth experience to install, update, and organize your favorite mods.

## Supporting development

r2modmac is not ad-free: it can occasionally show a short, clearly labelled **text-only sponsored message** to help fund development. There are no banner images, pop-ups, video ads, or interruptions to installing, updating, syncing, or playing.

Sponsored messages are enabled by default and can be disabled at any time from **Settings → Support r2modmac**. Disabling them immediately stops future sponsor requests and removes cached messages; every core feature of the app continues to work exactly the same.

We do not send your installed mods, profiles, local files, paths, configuration, or other app content to the sponsor service. See [Sponsored messages](docs/sponsored-messages.md) for the full data boundary.

## Features

- **Multi-Game Support**: Browse Thunderstore communities and manage mods for supported game loaders
- **Outer Wilds Support**: Full support for Outer Wilds mods via OWML — browse, install and launch directly from the app
- **Profile Management**: Create and manage multiple profiles for different mod setups
- **Browse Mode**: Explore and discover mods without creating a profile first
- **Import/Export**: Share your profiles with friends via codes or files
- **Custom Profile Images**: Add custom images to your profiles
- **Fast Search**: Intelligent caching system for instant searches
- **Modern Interface**: Clean and intuitive design optimized for macOS
- **Dependency Management**: Automatic installation of required dependencies
- **Smart Install Modes**:
  - **New Mode (Default)**: Install adds mods to your profile list. Download only when you click "Apply to Game" (saves disk space)
  - **Legacy Mode**: Mods download immediately to a local cache when you click Install (faster but uses more storage)

## Mod loader compatibility

Thunderstore hosts communities that use different mod loaders. A game appearing on Thunderstore does not by itself guarantee that r2modmac can install and launch its mods correctly.

| Mod loader | Support | Scope and notes |
| --- | :---: | --- |
| **BepInEx** | ✅ Supported | Standard Thunderstore/BepInEx package layouts, profile syncing, dependencies, updates and vanilla/modded switching. |
| **Lovely** | ✅ Supported | Balatro packages and the Lovely runtime, including the macOS launch flow. |
| **ReturnOfModding** | ✅ Supported | Risk of Rain Returns packages, loader installation, updates, syncing and Wine/CrossOver launch configuration. |
| **OWML** | ✅ Supported | Outer Wilds profiles and launching through OWML. OWML is supported separately from the standard Thunderstore loader flow. |
| **MelonLoader** | ❌ Not currently supported | Packages that require MelonLoader-specific installation or launch handling are not installed safely yet. |
| **Northstar** | ❌ Not currently supported | Titanfall 2/Northstar-specific package layouts and launch handling are not implemented yet. |
| **GDWeave** | ❌ Not currently supported | WEBFISHING/GDWeave-specific installation and launch handling are not implemented yet. |
| **Other custom loaders** | ❌ Not currently supported | Loader-specific layouts are unsupported until they receive an explicit integration and tests. |

“Supported” means that r2modmac understands the loader's package layout and handles Apply/Sync and launching where required. You can still browse an unsupported Thunderstore community, but you should not assume its packages can be installed correctly. If a loader is missing from this table, please [open an issue](https://github.com/Zard-Studios/r2modmac/issues) before relying on it.

## 📸 Screenshots

<div align="center">

### Game Selection
![Game Selection](https://github.com/user-attachments/assets/936d2fe6-3fd9-4359-9ab3-ef5b3266e0c4)

### Profile Management
![Profile Management](https://github.com/user-attachments/assets/2961952b-5dac-4eda-967e-0c425774b030)

### Browse Mods
![Mod Browser](https://github.com/user-attachments/assets/680d8bf7-4bf3-4102-87bf-132655803a61)
</div>

## Mod loader compatibility

These are the loaders currently handled by r2modmac. Other Thunderstore communities can still be browsed, but their mods may not install correctly.

| Loader | Status | Notes |
| --- | :---: | --- |
| **BepInEx** | ✅ Supported | Standard Thunderstore packages. |
| **Lovely** | ✅ Supported | Balatro mods and runtime. |
| **ReturnOfModding** | ✅ Supported | Risk of Rain Returns mods and runtime. |
| **OWML** | ✅ Supported | Outer Wilds; separate from the standard Thunderstore flow. |
| **MelonLoader** | ❌ Not supported | Loader-specific install and launch are not implemented. |
| **Northstar** | ❌ Not supported | Titanfall 2/Northstar integration is not implemented. |
| **GDWeave** | ❌ Not supported | WEBFISHING/GDWeave integration is not implemented. |
| **Other custom loaders** | ❌ Not supported | Requires a dedicated integration. |

If a loader is missing from this list, please [open an issue](https://github.com/Zard-Studios/r2modmac/issues) before relying on it.


## 🛠️ Technologies Used

### Frontend
- **React 19** - Modern UI framework with the latest features
- **TypeScript** - Type safety and better developer experience
- **Tailwind CSS** - Utility-first styling for consistent design
- **Zustand** - Lightweight and performant state management
- **Vite** - Lightning fast build tool for development and production

### Backend
- **Tauri 2** - Framework for native desktop applications using Rust
- **Rust** - Safe and performant language for backend logic
- **Reqwest** - HTTP client for communicating with Thunderstore API
- **Tokio** - Asynchronous runtime for non-blocking operations
- **Serde** - JSON serialization/deserialization

### Key Libraries
- **@tanstack/react-virtual** - List virtualization for optimal performance
- **adm-zip** - ZIP archive management for mods
- **js-yaml** - Profile configuration file parsing
- **regex** - Pattern matching for parsing and validation

## 📥 Installation

### Download
Download the latest version from the [releases page](https://github.com/Zard-Studios/r2modmac/releases).

### Troubleshooting

#### "The application is damaged and can't be opened"
If you see this error when opening the app, it's because it hasn't been signed with an Apple Developer certificate.

**Quick fix:**
```bash
sudo find /Applications/r2modmac.app -exec xattr -c {} \;
```

Enter your password when prompted, then try opening the app again.

## 🚀 Development

### Prerequisites
- Node.js 18+
- Rust 1.77+
- Xcode Command Line Tools

### Setup
```bash
# Clone the repository
git clone https://github.com/Zard-Studios/r2modmac.git
cd r2modmac

# Install dependencies
npm install

# Start in development mode
npm run dev

# Build for production
npm run tauri build
```

## 🤝 Contributing

Contributions are welcome! Feel free to:
- 🐛 Report bugs
- 💡 Propose new features
- 🔧 Submit pull requests

## 📝 License

This project is released under the MIT License. You can use, modify, and distribute it freely, as long as you maintain the original credits.

## 🙏 Acknowledgments

- [r2modman](https://github.com/ebkr/r2modmanPlus) - Inspiration for the project
- [Thunderstore](https://thunderstore.io/) - API for mods and community
- [OWML](https://github.com/ow-mods/owml) - Outer Wilds Mod Loader
- [Tauri](https://tauri.app/) - Framework for desktop applications

## ⭐ Star History

If you like the project, leave a star! ⭐

## Star History

<a href="https://www.star-history.com/?repos=Zard-Studios%2Fr2modmac&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=Zard-Studios/r2modmac&type=date&theme=dark&legend=top-left&sealed_token=78-5pTyzZjU7vMuRd6DkxEVOh9kR0-xOfzGllsK16AwZoXNFcMAY6IVtPjCnUfHx65fcn1oZNDRxWl5N768XOkD4GlmoMSmujkDWs-THQ2j3-Yaxd1HXzT7iHIITUt66psIiGq7X2feFzE6nBMn4OzHXJWxO5_Wfm1VgB4C_XPEX8EiV9tgPECAhDIDB" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=Zard-Studios/r2modmac&type=date&legend=top-left&sealed_token=78-5pTyzZjU7vMuRd6DkxEVOh9kR0-xOfzGllsK16AwZoXNFcMAY6IVtPjCnUfHx65fcn1oZNDRxWl5N768XOkD4GlmoMSmujkDWs-THQ2j3-Yaxd1HXzT7iHIITUt66psIiGq7X2feFzE6nBMn4OzHXJWxO5_Wfm1VgB4C_XPEX8EiV9tgPECAhDIDB" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=Zard-Studios/r2modmac&type=date&legend=top-left&sealed_token=78-5pTyzZjU7vMuRd6DkxEVOh9kR0-xOfzGllsK16AwZoXNFcMAY6IVtPjCnUfHx65fcn1oZNDRxWl5N768XOkD4GlmoMSmujkDWs-THQ2j3-Yaxd1HXzT7iHIITUt66psIiGq7X2feFzE6nBMn4OzHXJWxO5_Wfm1VgB4C_XPEX8EiV9tgPECAhDIDB" />
 </picture>
</a>

---

<div align="center">

r2modmac is and will always remain open-source. It has no analytics or telemetry, and it does not share personal data or application content. Optional text-only sponsored messages help support development and can always be disabled in Settings. I dedicate all my free time to it as a student and independent developer. If this tool has saved you hours of configuration and headaches, please consider supporting its development with a micro donation!

**Made with ❤️ for the modding community**

[Report Bug](https://github.com/Zard-Studios/r2modmac/issues) · [Request Feature](https://github.com/Zard-Studios/r2modmac/issues)

</div>
