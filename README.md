# r2modmac

<div align="center">

![r2modmac Icon](https://i.ibb.co/60j664sL/i-OS-Default-1024x1024-1x.png)

**Un mod manager moderno e nativo per macOS, ispirato a r2modman, riprogettato da 0**

[![GitHub release](https://img.shields.io/github/v/release/Zard-Studios/r2modmac)](https://github.com/Zard-Studios/r2modmac/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Stars](https://img.shields.io/github/stars/Zard-Studios/r2modmac?style=social)](https://github.com/Zard-Studios/r2modmac/stargazers)

</div>

## Descrizione

r2modmac è un mod manager nativo per macOS che permette di gestire facilmente le mod per giochi supportati da Thunderstore. Progettato con un'interfaccia moderna e intuitiva, offre un'esperienza fluida per installare, aggiornare e organizzare le tue mod preferite.

## Caratteristiche

- **Supporto Multi-Gioco**: Gestisci mod per tutti i giochi disponibili su Thunderstore
- **Gestione Profili**: Crea e gestisci profili multipli per diversi setup di mod
- **Import/Export**: Condividi i tuoi profili con amici tramite codici o file
- **Immagini Profilo Personalizzate**: Aggiungi immagini personalizzate ai tuoi profili
- **Ricerca Veloce**: Sistema di caching intelligente per ricerche istantanee
- **Interfaccia Moderna**: Design pulito e intuitivo ottimizzato per macOS
- **Gestione Dipendenze**: Installazione automatica delle dipendenze richieste

## 📸 Screenshots

<div align="center">

### Selezione Gioco
![Game Selection](https://i.ibb.co/r2JkH8Px/Screenshot-2025-12-03-alle-11-24-00.png)

### Gestione Profili
![Profile Management](https://i.ibb.co/CshMpFDJ/Screenshot-2025-12-03-alle-11-24-57.png)

### Browse Mods
![Mod Browser](https://i.ibb.co/n855t046/Screenshot-2025-12-03-alle-11-24-19.png)

</div>

## 🛠️ Tecnologie Utilizzate

### Frontend
- **React 19** - Framework UI moderno con le ultime funzionalità
- **TypeScript** - Type safety e migliore developer experience
- **Tailwind CSS** - Styling utility-first per un design consistente
- **Zustand** - State management leggero e performante
- **Vite** - Build tool velocissimo per sviluppo e produzione

### Backend
- **Tauri 2** - Framework per applicazioni desktop native usando Rust
- **Rust** - Linguaggio sicuro e performante per la logica backend
- **Reqwest** - HTTP client per comunicare con Thunderstore API
- **Tokio** - Runtime asincrono per operazioni non-bloccanti
- **Serde** - Serializzazione/deserializzazione JSON

### Librerie Chiave
- **@tanstack/react-virtual** - Virtualizzazione liste per performance ottimali
- **adm-zip** - Gestione archivi ZIP per mod
- **js-yaml** - Parsing file di configurazione profili
- **regex** - Pattern matching per parsing e validazione

## 📥 Installazione

### Download
Scarica l'ultima versione dalla [pagina releases](https://github.com/Zard-Studios/r2modmac/releases).

### Risoluzione Problemi

#### "L'applicazione è danneggiata e non può essere aperta"
Se vedi questo errore quando apri l'app, è perché non è stata firmata con un certificato Apple Developer.

**Soluzione rapida:**
```bash
sudo xattr -cr /Applications/r2modmac.app
```

Inserisci la password quando richiesto, poi prova ad aprire l'app di nuovo.

## 🚀 Sviluppo

### Prerequisiti
- Node.js 18+
- Rust 1.77+
- Xcode Command Line Tools

### Setup
```bash
# Clone il repository
git clone https://github.com/Zard-Studios/r2modmac.git
cd r2modmac

# Installa dipendenze
npm install

# Avvia in modalità sviluppo
npm run dev

# Build per produzione
npm run build
```

## 🤝 Contribuire

I contributi sono benvenuti! Sentiti libero di:
- 🐛 Segnalare bug
- 💡 Proporre nuove funzionalità
- 🔧 Inviare pull request

## 📝 Licenza

Questo progetto è rilasciato sotto licenza MIT. Puoi usarlo, modificarlo e distribuirlo liberamente, basta che mantieni i crediti originali.

## 🙏 Ringraziamenti

- [r2modman](https://github.com/ebkr/r2modmanPlus) - Ispirazione per il progetto
- [Thunderstore](https://thunderstore.io/) - API per mod e community
- [Tauri](https://tauri.app/) - Framework per applicazioni desktop

## ⭐ Star History

Se ti piace il progetto, lascia una stella! ⭐

[![Star History Chart](https://api.star-history.com/svg?repos=Zard-Studios/r2modmac&type=Date)](https://star-history.com/#Zard-Studios/r2modmac&Date)

---

<div align="center">

**Fatto con ❤️ per la community di modding**

[Report Bug](https://github.com/Zard-Studios/r2modmac/issues) · [Request Feature](https://github.com/Zard-Studios/r2modmac/issues)

</div>
