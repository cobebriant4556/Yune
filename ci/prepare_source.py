import os
from pathlib import Path

root = Path(os.environ['SOURCE'])
p = root / 'YTLitePlus.h'
s = p.read_text()
old = '@interface MLPlayerEventCenter : NSObject'
assert s.count(old) == 1
s = s.replace(old, '@interface MLPlayerEventCenter (YTLitePlusCompatibility)')
p.write_text(s)
p = root / 'Source/Settings.xm'
s = p.read_text()
for old, new in [
    ('sectionGestureSelectedModeToString(idx)', 'sectionGestureSelectedModeToString((GestureMode)idx)'),
    ('sectionGestureSelectedModeToString(GetInteger(sectionKey))', 'sectionGestureSelectedModeToString((GestureMode)GetInteger(sectionKey))'),
]:
    assert old in s, old
    s = s.replace(old, new)
p.write_text(s)
p = root / 'Makefile'
s = p.read_text().replace('export ADDITIONAL_CFLAGS = -I$(THEOS_PROJECT_DIR)/Tweaks', 'export ADDITIONAL_CFLAGS = -I$(THEOS_PROJECT_DIR)/Tweaks -Wno-error=vla-cxx-extension')
p.write_text('MAKEFLAGS += --keep-going\n' + s)
p = root / 'Tweaks/Alderis/lcpshim/Makefile'
p.write_text('TARGET = iphone:clang:16.5:12.0\n' + p.read_text())
p = Path(os.environ['THEOS']) / 'vendor/lib/CydiaSubstrate.framework/CydiaSubstrate.tbd'
p.write_text('''--- !tapi-tbd
tbd-version: 4
targets: [ arm64-ios ]
install-name: '/Library/Frameworks/CydiaSubstrate.framework/CydiaSubstrate'
current-version: 0.0.0
compatibility-version: 0.0.0
exports:
  - targets: [ arm64-ios ]
    symbols: [ _MSCloseImage, _MSDebug, _MSFindAddress, _MSFindSymbol, _MSGetImageByName, _MSHookClassPair, _MSHookFunction, _MSHookMemory, _MSHookMessageEx, _MSImageAddress, _MSMapImage ]
...
''')
print('Applied source and ARM64-device SDK/linker compatibility fixes; no tweak features removed.')
