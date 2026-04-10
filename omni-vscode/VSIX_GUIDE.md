# Omni VS Code Extension - VSIX Packaging Guide

Since you have changed the publisher to **XDess223**, you can package the extension into a `.vsix` file for easy distribution and manual installation.

## Prerequisites

You need the `@vscode/vsce` tool installed globally via npm:

```powershell
npm install -g @vscode/vsce
```

## How to Create the .vsix File

1.  Open a terminal in the `omni-vscode` directory.
2.  Run the following command:

```powershell
vsce package --no-dependencies
```

This will generate a file named `omni-lang-1.0.0.vsix` in the current directory.

## How to Install the .vsix File

### Option 1: Using the CLI (Recommended for Automation)

```powershell
code --install-extension .\omni-lang-1.0.0.vsix
```

### Option 2: Using the VS Code UI

1.  Open VS Code.
2.  Go to the **Extensions** view (`Ctrl+Shift+X`).
3.  Click the **...** (More Actions) menu in the top right of the Extensions view.
4.  Select **Install from VSIX...**.
5.  Choose the generated `.vsix` file.

---

> [!TIP]
> **Developer Automation:**
> I am providing a `refresh_extension.bat` script in the `omni-lang` directory that automates the uninstallation, packaging, and re-installation of your extension.
