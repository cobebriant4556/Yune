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
p.write_text(s)
print('Applied build compatibility fixes; no tweak features removed.')
