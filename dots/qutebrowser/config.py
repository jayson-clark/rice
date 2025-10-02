# Config.py
config.load_autoconfig(True)
import os
from pathlib import Path

# Resolve startpage path using the user's HOME so it's not hard-coded
home = Path.home()
startpage = str(home / '.config' / 'qutebrowser' / 'startpage.html')

# Adblock
# c.content.autoplay= False
# c.content.blocking.method = 'both'
# c.content.default_encoding = 'utf-8'
# c.content.geolocation = False

# Pdfs in qutebrowser
c.content.pdfjs = True

# Binds
config.bind('<Ctrl-=>', 'zoom-in')
config.bind('<Ctrl-->', 'zoom-out')

# Google
c.url.searchengines = {'DEFAULT': 'https://google.com/search?hl=en&q={}'}

# Default page
c.url.start_pages = [startpage]
c.url.default_page = startpage
c.tabs.last_close = 'startpage'

# Theme
# Theme - values from theme.json, updated via scripts/apply_theme.py
c.colors.hints.bg = '#e6eef6'
c.colors.hints.fg = '#0f1117'
c.hints.border = '1px solid #000000'
c.colors.webpage.preferred_color_scheme = 'dark'
c.colors.webpage.bg = '#0f1117'