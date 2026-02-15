# -*- mode: python ; coding: utf-8 -*-


a = Analysis(
    ['src-python\\main.py'],
    pathex=[],
    binaries=[],
    datas=[('.venv.build\\Lib\\site-packages\\pykakasi\\data', 'pykakasi\\data')],
    hiddenimports=[],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=['torch', 'IPython', 'notebook', 'matplotlib', 'tkinter', 'PIL', 'pandas'],
    noarchive=False,
    optimize=2,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [('O', None, 'OPTION'), ('O', None, 'OPTION')],
    name='translator-x86_64-pc-windows-msvc',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=True,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon='src-tauri\\icons\\icon.ico'
)
