# Einstellungen für dmgbuild (tools/release.sh): Fenster, Hintergrund und Icon-Plätze des DMG.
# Aufruf: dmgbuild -s tools/dmg/settings.py -D app=<Kadrell.app> -D background=<png> -D icon=<icns> "<Volume>" <out.dmg>
# Koordinaten in pt, Ursprung oben links, Icon-Mitte. Passen zu Lichtkegeln und Winkeln in background.swift.
import os.path

app = defines["app"]  # noqa: F821, von dmgbuild gesetzt
format = "UDZO"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = defines["icon"]  # noqa: F821
background = defines["background"]  # noqa: F821
window_rect = ((200, 160), (660, 420))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
include_icon_view_settings = True
icon_size = 128
text_size = 13
arrange_by = None
icon_locations = {os.path.basename(app): (170, 225), "Applications": (490, 225)}
