# VS Code Extension Refresh Guide

If you are seeing white text (no colors) in your `.omni` files, follow these steps to perform a clean refresh of the Omni Language extension.

---

### 🛠️ Step 1: Uninstall the Old Extension
1. Open VS Code.
2. Press `Ctrl + Shift + X` to open the **Extensions** panel.
3. Search for **Omni Language**.
4. Click **Uninstall**.

### 🧹 Step 2: Clear the Extension Cache
VS Code sometimes caches old grammar files. To ensure you get the new version:
1. Close all VS Code windows.
2. Clear the extension directory (Optional but recommended):
   - **Windows**: `%USERPROFILE%\.vscode\extensions`
   - Delete any folder starting with `antigravity.omni-lang`.

### 📦 Step 3: Install the Fresh VSIX
1. Open your terminal in the `omni-lang` directory.
2. Run the following command:
   ```powershell
   code --install-extension .\omni-vscode\omni-lang-1.0.0.vsix
   ```
3. Alternatively, in VS Code:
   - Go to the **Extensions** panel.
   - Click the `...` (More Actions) in the top right.
   - Select **Install from VSIX...**.
   - Choose the file in `.\omni-vscode\omni-lang-1.0.0.vsix`.

### 🔄 Step 4: Final Reload
1. Press `Ctrl + Shift + P` to open the Command Palette.
2. Type **Developer: Reload Window** and press Enter.

---

### ✅ Verification
Open an `.omni` file (like `examples/closures.omni`). Keywords like `class`, `function`, and `forall` should now be colored. 
