# r2modmac

<p align="center">
  <img src="https://i.ibb.co/60j664sL/i-OS-Default-1024x1024-1x.png" alt="r2modmac-icon" width="200" height="200">
  <br>
  <strong>A modern and native mod manager for macOS, inspired by r2modman, redesigned from scratch</strong>
  <br><br>
  <a href="https://github.com/Zard-Studios/r2modmac/releases">
    <img src="https://img.shields.io/github/v/release/Zard-Studios/r2modmac" alt="GitHub release">
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

r2modmac is a native mod manager for macOS that allows you to easily manage mods for Thunderstore supported games. Designed with a modern and intuitive interface, it offers a smooth experience to install, update, and organize your favorite mods.

## Features

- **Multi-Game Support**: Manage mods for all games available on Thunderstore
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

## 📸 Screenshots

<div align="center">

### Game Selection
![Game Selection](https://github.com/user-attachments/assets/29a4b61b-042f-456e-b33a-60a0c4b84174)

### Profile Management
![Profile Management](https://github.com/user-attachments/assets/a17cc052-3ac1-45ad-a9e6-24a0fac7c1c3)

### Browse Mods
![Mod Browser](https://github.com/user-attachments/assets/2ff6049f-2494-46dc-91a8-6f2e8d1cb238)
</div>


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
- [Tauri](https://tauri.app/) - Framework for desktop applications

## ⭐ Star History

If you like the project, leave a star! ⭐

[![Star History Chart](https://api.star-history.com/svg?repos=Zard-Studios/r2modmac&type=Date)](https://star-history.com/#Zard-Studios/r2modmac&Date)

---

<div align="center">

**Made with ❤️ for the modding community**

[Report Bug](https://github.com/Zard-Studios/r2modmac/issues) · [Request Feature](https://github.com/Zard-Studios/r2modmac/issues)

</div>
