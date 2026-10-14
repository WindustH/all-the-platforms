#!/usr/bin/env python3
"""Draw the platform grid in the README: docs/assets/platforms-{light,dark}.svg.

Brand glyphs come from Simple Icons (CC0) at a pinned version; brands it
lacks use their official App Store icon or GitHub organization avatar. All
are fetched when this runs. Edit the tables below when platforms are added,
then run:

    python3 docs/assets/platforms.py
"""

import base64
import json
import re
import urllib.request
from html import escape
from pathlib import Path

SIMPLE_ICONS = "https://cdn.jsdelivr.net/npm/simple-icons@16.35.0/icons/{}.svg"
APP_STORE = "https://itunes.apple.com/lookup?id={}&country={}"
GITHUB_AVATAR = "https://github.com/{}.png?size=96"
OUT = Path(__file__).resolve().parent

# (command, name, latin name, icon, tile color). The icon is a Simple Icons
# slug drawn in white (or black) on the tile color, `app:ID[@country]` for
# the App Store icon of that app, or `github:ORG` for an organization avatar.
GROUPS = [
    ("社交", "SOCIAL", [
        ("zhihu", "知乎", "Zhihu", "zhihu", "#0084FF"),
        ("xhs", "小红书", "RedNote", "xiaohongshu", "#FF2442"),
        ("weibo", "微博", "Weibo", "sinaweibo", "#E6162D"),
        ("tieba", "百度贴吧", "Tieba", "app:477927812", ""),
        ("toutiao", "今日头条", "Toutiao", "app:529092160", ""),
        ("mp", "公众号", "WeChat OA", "wechat", "#07C160"),
        ("twitter", "X", "Twitter", "x", "#000000"),
        ("reddit", "Reddit", "Reddit", "reddit", "#FF4500"),
        ("facebook", "Facebook", "Facebook", "facebook", "#0866FF"),
        ("instagram", "Instagram", "Instagram", "instagram", "instagram"),
        ("misskey", "Misskey", "Misskey", "misskey", "#A1CA03"),
    ]),
    ("视频", "VIDEO", [
        ("bili", "哔哩哔哩", "Bilibili", "bilibili", "#FB7299"),
        ("douyin", "抖音", "Douyin", "tiktok", "tiktok"),
        ("youtube", "YouTube", "YouTube", "youtube", "#FF0000"),
        ("tiktok", "TikTok", "TikTok", "tiktok", "tiktok"),
        ("kuaishou", "快手", "Kuaishou", "kuaishou", "#FF4906"),
    ]),
    ("购物与生活", "SHOPPING", [
        ("taobao", "淘宝", "Taobao", "taobao", "#FF5000"),
        ("xianyu", "闲鱼", "Xianyu", "app:510909506", ""),
        ("jd", "京东", "JD.com", "app:414245413", ""),
        ("pdd", "拼多多", "Pinduoduo", "app:1044283059", ""),
        ("dewu", "得物", "Poizon", "app:1012871328", ""),
        ("meituan", "美团", "Meituan", "meituan", "#FFD100"),
        ("dianping", "大众点评", "Dianping", "app:351091731", ""),
    ]),
    ("地图与出行", "MAPS · TRAVEL", [
        ("amap", "高德地图", "Amap", "app:461703208", ""),
        ("ctrip", "携程", "Ctrip", "app:379395415", ""),
    ]),
    ("网盘", "DRIVES", [
        ("quark", "夸克网盘", "Quark Drive", "app:6755949705", ""),
        ("baidupan", "百度网盘", "Baidu Netdisk", "app:547166701", ""),
    ]),
    ("聊天与办公", "CHAT", [
        ("wechat", "微信", "WeChat ClawBot", "wechat", "#07C160"),
        ("telegram", "Telegram", "Telegram", "telegram", "#26A5E4"),
        ("discord", "Discord", "Discord", "discord", "#5865F2"),
        ("whatsapp", "WhatsApp", "WhatsApp", "whatsapp", "#25D366"),
        ("qq", "QQ", "QQ bots", "qq", "#12B7F5"),
        ("onebot", "OneBot", "NapCat · Lagrange", "github:botuniverse", ""),
        ("feishu", "飞书", "Feishu · Lark", "app:1401729613", ""),
        ("dingtalk", "钉钉", "DingTalk", "app:930368978", ""),
        ("wecom", "企业微信", "WeCom", "app:1087897068", ""),
        ("slack", "Slack", "Slack", "app:618783545@us", ""),
        ("mattermost", "Mattermost", "Mattermost", "mattermost", "#0058CC"),
        ("rocketchat", "Rocket.Chat", "Rocket.Chat", "rocketdotchat", "#F5455C"),
        ("matrix", "Matrix", "Matrix", "matrix", "#000000"),
        ("kook", "KOOK", "KOOK", "app:1501289327", ""),
        ("vocechat", "VoceChat", "VoceChat", "app:1631779678@us", ""),
        ("line", "LINE", "LINE", "line", "#00C300"),
        ("satori", "Satori", "Satori", "github:satorijs", ""),
    ]),
]

COMING = [
    ("Google Maps", "Google Maps", "googlemaps", "#4285F4"),
    ("百度地图", "Baidu Map", "app:452186370", ""),
    ("腾讯地图", "Tencent Map", "app:481623196", ""),
    ("Google", "Google Search", "google", "#4285F4"),
    ("Bing", "Bing", "app:345323231@us", ""),
    ("DuckDuckGo", "DuckDuckGo", "duckduckgo", "#DE5833"),
    ("SearXNG", "SearXNG", "searxng", "#3050FF"),
    ("Stack Overflow", "Stack Overflow", "stackoverflow", "#F58025"),
    ("Hacker News", "Hacker News", "ycombinator", "#F0652F"),
    ("Lobsters", "Lobsters", "lobsters", "#AC130D"),
    ("V2EX", "V2EX", "v2ex", "#1F1F1F"),
    ("Bangumi", "番组计划", "github:bangumi", ""),
    ("播客", "Podcasts", "applepodcasts", "#9933CC"),
    ("Lemmy", "Lemmy", "lemmy", "#000000"),
    ("RSS", "RSSHub · Miniflux", "rss", "#FFA500"),
    ("Threads", "Threads", "threads", "#000000"),
    ("Mastodon", "Mastodon", "mastodon", "#6364FF"),
    ("Bluesky", "Bluesky", "bluesky", "#1185FE"),
    ("Discourse", "linux.do", "discourse", "#000000"),
    ("豆瓣", "Douban", "douban", "#2D963D"),
    ("微信读书", "WeRead", "weread", "#37A7FF"),
    ("掘金", "Juejin", "juejin", "#007FFF"),
    ("小宇宙", "Xiaoyuzhou", "app:1488894313", ""),
    ("即刻", "Jike", "app:966129812", ""),
]

THEMES = {
    "dark": dict(panel="#16181D", chip="#1F232B", line="#2C313B", text="#E6E8EB",
                 sub="#959DA8", accent="#E08A5F", prompt="#7D8590", tile_line="#3A404C"),
    "light": dict(panel="#FBFAF8", chip="#F1EEEA", line="#E2DDD6", text="#1F2328",
                  sub="#6A6F76", accent="#B8582E", prompt="#8C959F", tile_line="#D6D0C8"),
}

SANS = "'PingFang SC','Hiragino Sans GB','Microsoft YaHei','Noto Sans CJK SC','Source Han Sans SC',system-ui,sans-serif"
MONO = "ui-monospace,SFMono-Regular,Menlo,Consolas,'Noto Sans Mono CJK SC',monospace"

WIDTH, PAD, LABEL_W = 1100, 36, 150
ICON, CHIP_H, GAP_X, GAP_Y = 26, 38, 10, 10
# The mitochondrion of docs/assets/atp-icon.svg: outer and inner membrane.
LOGO = (
    "M 30,64 C 40,24 98,8 134,32 C 152,44 170,42 196,48 C 232,56 244,96 232,126 "
    "C 218,158 170,164 126,154 C 96,147 74,158 48,148 C 12,134 20,94 30,64 Z"
)
CRISTAE = (
    "M 52.2,102.3 C 46.8,81.2 52.2,60 64.8,53 C 79.2,46 84.6,63.6 81,81.2 "
    "C 77.4,98.8 88.2,111.1 99,98.8 C 109.8,86.4 104.4,60 117,53 C 129.6,46 135,65.3 129.6,86.4 "
    "C 124.2,107.6 135,118.1 145.8,105.8 C 156.6,93.5 151.2,65.3 163.8,60 "
    "C 176.4,54.8 181.8,74.1 178.2,93.5 C 174.6,112.8 183.6,118.1 192.6,107.6 "
    "C 198,98.8 196.2,81.2 201.6,76.8 C 208.8,72.4 212.4,93.5 207,114.6 "
    "C 199.8,134 172.8,139.2 136.8,137.5 C 104.4,134 82.8,141 64.8,130.4 "
    "C 55.8,124.3 54,112.8 52.2,102.3 Z"
)


def glyph(slug, cache={}):
    if slug not in cache:
        with urllib.request.urlopen(SIMPLE_ICONS.format(slug), timeout=30) as r:
            svg = r.read().decode()
        cache[slug] = re.search(r' d="([^"]+)"', svg).group(1)
    return cache[slug]


def light(color):
    r, g, b = (int(color[i:i + 2], 16) for i in (1, 3, 5))
    return 0.299 * r + 0.587 * g + 0.114 * b > 170


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "atp-docs"})
    with urllib.request.urlopen(request, timeout=30) as r:
        return r.read(), r.headers.get_content_type()


def picture(icon, cache={}):
    """An App Store icon or a GitHub avatar as a data URI (96 px)."""
    if icon not in cache:
        kind, _, ref = icon.partition(":")
        if kind == "app":
            app, _, country = ref.partition("@")
            info = json.loads(fetch(APP_STORE.format(app, country or "cn"))[0])
            art = info["results"][0]["artworkUrl512"]
            url = re.sub(r"/\d+x\d+bb\.\w+$", "/96x96bb.jpg", art)
        else:
            url = GITHUB_AVATAR.format(ref)
        data, mime = fetch(url)
        cache[icon] = f"data:{mime};base64,{base64.b64encode(data).decode()}"
    return cache[icon]


def tile(x, y, icon, color, t, size):
    """A square app icon: a fetched picture, or a brand glyph on its color."""
    corner = f'rx="{size * 0.24:.1f}"'
    box = f'x="{x:.1f}" y="{y:.1f}" width="{size}" height="{size}"'
    if ":" in icon:
        clip = "clip-" + re.sub(r"\W", "-", icon)
        return (f'<clipPath id="{clip}"><rect {box} {corner}/></clipPath>'
                f'<image href="{picture(icon)}" {box} clip-path="url(#{clip})"/>'
                f'<rect {box} {corner} fill="none" stroke="{t["tile_line"]}" stroke-opacity=".6"/>')
    if color == "instagram":
        fill, ink = "url(#ig)", "#FFFFFF"
    elif color == "tiktok":
        fill, ink = "#000000", "#FFFFFF"
    else:
        fill, ink = color, "#111111" if light(color) else "#FFFFFF"
    dark_tile = color in ("tiktok", "#000000", "#111111", "#1F1F1F")
    stroke = f' stroke="{t["tile_line"]}"' if dark_tile else ""
    out = [f'<rect {box} {corner} fill="{fill}"{stroke}/>']
    d = glyph(icon)
    scale = size * 0.6 / 24
    offset = (size - 24 * scale) / 2

    def path(dx, dy, color_):
        return (f'<path transform="translate({x + offset + dx:.1f} {y + offset + dy:.1f}) '
                f'scale({scale:.3f})" fill="{color_}" d="{d}"/>')

    if color == "tiktok":
        out.append(path(-0.9, -0.9, "#25F4EE"))
        out.append(path(0.9, 0.9, "#FE2C55"))
    out.append(path(0, 0, ink))
    return "".join(out)


def width_of(text, size, mono=False):
    """A rough rendered width: CJK is square, Latin about half to 0.6 em."""
    latin = 0.6 if mono else 0.56
    return sum(size if ord(c) > 0x2E80 else size * latin for c in text)


def chip(x, y, command, name, icon, color, t, soon=False):
    """One entry: icon, the command (the name when not shipped yet) and a label."""
    title = name if soon else command
    same = re.sub(r"\W", "", name.lower()) == command
    label = "" if soon or same else name
    w = 6 + ICON + 9 + width_of(title, 14, mono=not soon) + 14
    if label:
        w += width_of(label, 13) + 8
    style = (f'fill="none" stroke="{t["line"]}" stroke-dasharray="4 4"' if soon
             else f'fill="{t["chip"]}"')
    out = [f'<g opacity="{0.62 if soon else 1}">',
           f'<rect x="{x:.0f}" y="{y}" width="{w:.0f}" height="{CHIP_H}" rx="{CHIP_H / 2}" {style}/>',
           tile(x + 6, y + (CHIP_H - ICON) / 2, icon, color, t, ICON)]
    tx = x + 6 + ICON + 9
    font = SANS if soon else MONO
    out.append(f'<text x="{tx:.0f}" y="{y + 24}" fill="{t["text"]}" font-family="{font}" '
               f'font-size="14" font-weight="600">{escape(title)}</text>')
    if label:
        lx = tx + width_of(title, 14, mono=True) + 8
        out.append(f'<text x="{lx:.0f}" y="{y + 24}" fill="{t["sub"]}" font-family="{SANS}" '
                   f'font-size="13">{escape(label)}</text>')
    out.append("</g>")
    return "".join(out), w


def flow(entries, y, t, soon=False):
    """Chips left to right, wrapping inside the column next to the labels."""
    x0, right = PAD + LABEL_W, WIDTH - PAD
    x, parts = x0, []
    for entry in entries:
        svg, w = chip(x, y, *entry, t, soon=soon)
        if x > x0 and x + w > right:
            x, y = x0, y + CHIP_H + GAP_Y
            svg, w = chip(x, y, *entry, t, soon=soon)
        parts.append(svg)
        x += w + GAP_X
    return "".join(parts), y + CHIP_H


def label(y, zh, en, count, t, color=None):
    return (f'<text x="{PAD}" y="{y + 17}" fill="{color or t["text"]}" font-family="{SANS}" '
            f'font-size="15" font-weight="700">{escape(zh)}</text>'
            f'<text x="{PAD}" y="{y + 35}" fill="{t["sub"]}" font-family="{MONO}" '
            f'font-size="11.5">{escape(en.lower())} · {count}</text>')


def heading(t, shipped, soon):
    logo = (f'<g transform="translate({PAD - 2} 28) scale(0.16) rotate(-10 120 88)">'
            f'<path d="{LOGO}" fill="#CF6F42"/><path d="{CRISTAE}" fill="none" '
            f'stroke="{t["panel"]}" stroke-width="10" stroke-linejoin="round"/></g>')
    prompt = (f'<text x="{PAD + 44}" y="52" font-family="{MONO}" font-size="17">'
              f'<tspan fill="{t["accent"]}">$</tspan>'
              f'<tspan dx="10" fill="{t["text"]}" font-weight="600">atp platforms</tspan></text>')
    summary = (f'<text x="{WIDTH - PAD}" y="52" text-anchor="end" fill="{t["prompt"]}" '
               f'font-family="{MONO}" font-size="13">{shipped} supported · {soon} on the way</text>')
    rule = f'<line x1="{PAD}" y1="76" x2="{WIDTH - PAD}" y2="76" stroke="{t["line"]}"/>'
    return logo + prompt + summary + rule


def draw(t):
    shipped = sum(len(items) for _, _, items in GROUPS)
    parts, y = [heading(t, shipped, len(COMING))], 98
    for zh, en, items in GROUPS:
        parts.append(label(y, zh, en, len(items), t))
        block, bottom = flow([(c, n, i, col) for c, n, _, i, col in items], y, t)
        parts.append(block)
        y = max(bottom, y + 40) + 24
    parts.append(f'<line x1="{PAD}" y1="{y - 6}" x2="{WIDTH - PAD}" y2="{y - 6}" '
                 f'stroke="{t["line"]}" stroke-dasharray="2 6"/>')
    y += 16
    parts.append(label(y, "即将支持", "Coming", len(COMING), t, t["accent"]))
    block, bottom = flow([("", n, i, col) for n, _, i, col in COMING], y, t, soon=True)
    parts.append(block)
    height = bottom + PAD
    defs = ('<defs><linearGradient id="ig" x1="0" y1="1" x2="1" y2="0">'
            '<stop offset="0" stop-color="#FEDA75"/><stop offset=".3" stop-color="#FA7E1E"/>'
            '<stop offset=".6" stop-color="#D62976"/><stop offset="1" stop-color="#4F5BD5"/>'
            '</linearGradient></defs>')
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height:.0f}" '
            f'viewBox="0 0 {WIDTH} {height:.0f}" role="img" aria-label="Supported platforms">{defs}'
            f'<rect width="{WIDTH}" height="{height:.0f}" rx="18" fill="{t["panel"]}"/>'
            + "".join(parts) + "</svg>\n")


def main():
    for name, theme in THEMES.items():
        (OUT / f"platforms-{name}.svg").write_text(draw(theme))
        print("wrote", OUT / f"platforms-{name}.svg")


if __name__ == "__main__":
    main()
