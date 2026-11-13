# RustSearch 🦀🔍

**Un buscador de archivos ultrarrápido para Windows, inspirado en [Everything](https://www.voidtools.com/) y escrito en Rust.**
**An ultra-fast file search for Windows, inspired by [Everything](https://www.voidtools.com/) and written in Rust.**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-blue.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-Windows-lightgrey.svg)](https://www.microsoft.com/windows)

[![PayPal](https://img.shields.io/badge/Donar-PayPal-blue.svg)](https://www.paypal.com/paypalme/RutaCun)
[![Mercado Pago](https://img.shields.io/badge/Donar-Mercado%20Pago-blue.svg)](https://link.mercadopago.com.mx/rutacun)

---

## 🇪🇸 Español

### 📖 ¿Qué es RustSearch?

**RustSearch** es una aplicación de escritorio para Windows que te permite encontrar cualquier archivo o carpeta en tu computadora de forma **instantánea**. A diferencia del buscador tradicional de Windows, que puede tardar minutos, RustSearch indexa tu disco duro en segundo plano y te muestra los resultados mientras escribes, sin interrupciones.

Este proyecto nació como un experimento personal de **"vibe coding"** con IA, con el objetivo de crear una alternativa moderna, ligera y de código abierto a Everything, aprovechando la velocidad y seguridad del lenguaje **Rust**.

### ✨ Características Principales

- ⚡ **Búsqueda en tiempo real:** Resultados instantáneos por nombre o ruta.
- 💾 **Indexación persistente:** Guarda una caché en disco, por lo que el arranque es casi inmediato tras la primera vez.
- 📊 **Tabla virtualizada:** Diseñada para manejar cientos de miles de archivos sin perder fluidez. Incluye ordenación por columnas.
- 🗑️ **Papelera interna:** Los archivos eliminados van a una papelera propia de RustSearch, desde donde puedes **restaurarlos** (Ctrl+Z) o vaciarla permanentemente.
- 🎨 **Interfaz moderna:** Efecto acrílico (cristal) nativo de Windows 11, con un diseño oscuro y minimalista.
- ⌨️ **Atajos de teclado:** Copiar, eliminar, restaurar y refrescar sin tocar el ratón.
- 📂 **Menú contextual:** Abrir archivo, abrir ubicación, copiar ruta, eliminar, etc.

### 🚀 Instalación y Uso

#### Opción 1: Descargar el Ejecutable (Recomendado)

1. Ve a la sección de [**Releases**](https://github.com/SoyMaanuel/RustSearch/releases) de este repositorio.
2. Descarga el archivo `RustSearch.exe` de la última versión.
3. Ejecútalo. ¡Listo! No requiere instalación.

> **Nota:** Al ser un ejecutable sin firma digital, Windows SmartScreen podría mostrar una advertencia. Haz clic en "Más información" y luego en "Ejecutar de todas formas".

#### Opción 2: Compilar desde el Código Fuente

Si eres desarrollador y quieres compilarlo tú mismo, necesitas tener instalado [Rust](https://www.rust-lang.org/tools/install).

```bash
git clone https://github.com/SoyMaanuel/RustSearch.git
cd RustSearch
cargo build --release
```

El ejecutable se generará en `target/release/rustsearch.exe`.

### ⌨️ Atajos de Teclado

| Atajo | Acción |
|-------|--------|
| `Ctrl + C` | Copiar el nombre o la ruta del archivo seleccionado. |
| `Supr` | Mover el archivo a la papelera interna. |
| `Shift + Supr` | Eliminar el archivo permanentemente. |
| `Ctrl + Z` | Restaurar el último archivo eliminado. |
| `F5` | Refrescar el índice (útil si has añadido archivos nuevos). |
| `Flechas ↑/↓` | Navegar por la lista de resultados. |
| `Enter` | Abrir el archivo o la carpeta seleccionada. |

### 🤝 Cómo Contribuir

¡Las contribuciones son bienvenidas! Si tienes ideas, encuentras errores o quieres mejorar el código:

1. Abre un **Issue** para reportar un bug o sugerir una mejora.
2. Haz un **Fork** del proyecto.
3. Crea una rama para tu feature (`git checkout -b feature/nueva-funcionalidad`).
4. Haz commit de tus cambios (`git commit -m 'Añade nueva funcionalidad'`).
5. Haz push a la rama (`git push origin feature/nueva-funcionalidad`).
6. Abre un **Pull Request**.

### ❤️ Apoya el Proyecto

Si RustSearch te resulta útil, puedes invitarme un café o apoyar el mantenimiento del proyecto. ¡Cada aporte ayuda muchísimo!

[![PayPal](https://www.paypalobjects.com/en_US/i/btn/btn_donateCC_LG.gif)](https://www.paypal.com/paypalme/RutaCun)
[![Mercado Pago](https://img.shields.io/badge/Donar-Mercado%20Pago-blue?style=for-the-badge&logo=mercadopago)](https://link.mercadopago.com.mx/rutacun)

### 📄 Licencia

Este proyecto está bajo la **Licencia MIT**. Consulta el archivo `LICENSE` para más detalles.

### 📬 Contacto

**Manuel Hernandez (SoyMaanuel)**

- 📧 Email: soymaanuel20@gmail.com
- 🐙 GitHub: [@SoyMaanuel](https://github.com/SoyMaanuel)
- 📍 Cancún, Quintana Roo, México

---

## 🇬🇧 English

### 📖 What is RustSearch?

**RustSearch** is a desktop application for Windows that lets you find any file or folder on your computer **instantly**. Unlike the traditional Windows search, which can take minutes, RustSearch indexes your hard drive in the background and shows you results as you type, without interruptions.

This project was born as a personal **"vibe coding"** experiment with AI, aiming to create a modern, lightweight, and open-source alternative to Everything, leveraging the speed and safety of the **Rust** language.

### ✨ Key Features

- ⚡ **Real-time search:** Instant results by name or path.
- 💾 **Persistent indexing:** Saves a cache to disk, so startup is nearly instant after the first run.
- 📊 **Virtualized table:** Designed to handle hundreds of thousands of files without losing fluidity. Includes column sorting.
- 🗑️ **Internal trash:** Deleted files go to RustSearch's own trash, where you can **restore them** (Ctrl+Z) or empty it permanently.
- 🎨 **Modern UI:** Native Windows 11 acrylic (blur) effect, with a dark and minimalist design.
- ⌨️ **Keyboard shortcuts:** Copy, delete, restore, and refresh without touching the mouse.
- 📂 **Context menu:** Open file, open location, copy path, delete, etc.

### 🚀 Installation & Usage

#### Option 1: Download the Executable (Recommended)

1. Go to the [**Releases**](https://github.com/SoyMaanuel/RustSearch/releases) section of this repository.
2. Download the `RustSearch.exe` file from the latest version.
3. Run it. That's it! No installation required.

> **Note:** As an unsigned executable, Windows SmartScreen may show a warning. Click "More info" and then "Run anyway".

#### Option 2: Build from Source

If you are a developer and want to build it yourself, you need [Rust](https://www.rust-lang.org/tools/install) installed.

```bash
git clone https://github.com/SoyMaanuel/RustSearch.git
cd RustSearch
cargo build --release
```

The executable will be generated at `target/release/rustsearch.exe`.

### ⌨️ Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl + C` | Copy the name or path of the selected file. |
| `Del` | Move the file to the internal trash. |
| `Shift + Del` | Permanently delete the file. |
| `Ctrl + Z` | Restore the last deleted file. |
| `F5` | Refresh the index (useful if you've added new files). |
| `Arrow ↑/↓` | Navigate through the results list. |
| `Enter` | Open the selected file or folder. |

### 🤝 Contributing

Contributions are welcome! If you have ideas, find bugs, or want to improve the code:

1. Open an **Issue** to report a bug or suggest an improvement.
2. **Fork** the project.
3. Create a branch for your feature (`git checkout -b feature/new-feature`).
4. Commit your changes (`git commit -m 'Add new feature'`).
5. Push to the branch (`git push origin feature/new-feature`).
6. Open a **Pull Request**.

### ❤️ Support the Project

If RustSearch is useful to you, you can buy me a coffee or support the project's maintenance. Every contribution helps a lot!

[![PayPal](https://www.paypalobjects.com/en_US/i/btn/btn_donateCC_LG.gif)](https://www.paypal.com/paypalme/RutaCun)
[![Mercado Pago](https://img.shields.io/badge/Donate-Mercado%20Pago-blue?style=for-the-badge&logo=mercadopago)](https://link.mercadopago.com.mx/rutacun)

### 📄 License

This project is licensed under the **MIT License**. See the `LICENSE` file for details.

### 📬 Contact

**Manuel Hernandez (SoyMaanuel)**

- 📧 Email: soymaanuel20@gmail.com
- 🐙 GitHub: [@SoyMaanuel](https://github.com/SoyMaanuel)
- 📍 Cancún, Quintana Roo, Mexico