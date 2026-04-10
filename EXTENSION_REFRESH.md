# Omni VS Code Extension: Developer Guide

This guide explains how to build and refresh your VS Code extension for the Omni language, especially after changing the publisher to `XDess223`.

## Prerequisites

1. **Node.js & npm**: Installed on your system.
2. **VSCE Tool**: The Visual Studio Code Extension Manager.
   ```powershell
   npm install -g @vscode/vsce
   ```

## Configuration

Ensure your `package.json` has the correct publisher:
```json
{
    "name": "omni-lang",
    "publisher": "XDess223",
    ...
}
```

## How to Build the Extension (.vsix)

The `.vsix` file is the package used to install the extension in VS Code.

1. Navigate to the `omni-vscode` directory.
2. Run the package command:
   ```powershell
   vsce package
   ```
   This will generate a file named `omni-lang-1.0.0.vsix`.

## Automated Refresh Script

Use the provided `refresh_extension.bat` to automate the process of cleaning up old versions, rebuilding, and reinstalling the extension.

### Using `refresh_extension.bat`

Run the script from the root `omni-lang` directory:
```powershell
.\refresh_extension.bat
```

The script performs the following:
1. Installs `vsce` if missing.
2. Navigates to `omni-vscode`.
3. Deletes any existing `.vsix` files.
4. Packages the new version.
5. Uninstalls the previous version from VS Code.
6. Installs the new `.vsix`.

## Troubleshooting

- **Extension not working?** 
  Try restarting VS Code or running "Developer: Reload Window" from the Command Palette (`Ctrl+Shift+P`).
- **Publisher Mismatch?**
  If you get an error that the extension ID is wrong, ensure the `publisher` field in `package.json` exactly matches the one you used in the `code --install-extension` command.
