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
    ("地图、出行与网盘", "MAPS · TRAVEL · DRIVES", [
        ("amap", "高德地图", "Amap", "app:461703208", ""),
        ("ctrip", "携程", "Ctrip", "app:379395415", ""),
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
    "dark": dict(bg="#13151A", panel="#1A1D24", card="#20242C", line="#2E333D",
                 text="#E8EAED", sub="#A2A9B4", mono="#6E7681", head="#8B949E",
                 coming="#E3B341", tile_line="#3A404C"),
    "light": dict(bg="#FFFFFF", panel="#F6F7F9", card="#FFFFFF", line="#E1E4E8",
                  text="#1F2328", sub="#57606A", mono="#8C959F", head="#57606A",
                  coming="#9A6700", tile_line="#D0D7DE"),
}

SANS = "'PingFang SC','Hiragino Sans GB','Microsoft YaHei','Noto Sans CJK SC','Source Han Sans SC',system-ui,sans-serif"
MONO = "ui-monospace,SFMono-Regular,Menlo,Consolas,monospace"

COLS, CARD_W, CARD_H, GAP, PAD = 5, 200, 76, 14, 32
WIDTH = PAD * 2 + COLS * CARD_W + (COLS - 1) * GAP
TILE = 44


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


def tile(x, y, icon, color, t):
    out = []
    if ":" in icon:
        clip = "clip-" + re.sub(r"\W", "-", icon)
        return (f'<clipPath id="{clip}"><rect x="{x}" y="{y}" width="{TILE}" height="{TILE}" rx="11"/></clipPath>'
                f'<image href="{picture(icon)}" x="{x}" y="{y}" width="{TILE}" height="{TILE}" clip-path="url(#{clip})"/>'
                f'<rect x="{x}" y="{y}" width="{TILE}" height="{TILE}" rx="11" fill="none" stroke="{t["tile_line"]}" stroke-opacity=".6"/>')
    if color == "instagram":
        fill, ink = "url(#ig)", "#FFFFFF"
    elif color == "tiktok":
        fill, ink = "#000000", "#FFFFFF"
    else:
        fill, ink = color, "#111111" if light(color) else "#FFFFFF"
    dark_tile = color in ("tiktok", "#000000", "#111111", "#1F1F1F")
    stroke = f' stroke="{t["tile_line"]}"' if dark_tile else ""
    out.append(f'<rect x="{x}" y="{y}" width="{TILE}" height="{TILE}" rx="11" fill="{fill}"{stroke}/>')
    d = glyph(icon)
    scale = (TILE - 18) / 24
    offset = (TILE - 24 * scale) / 2

    def path(dx, dy, color_):
        return (f'<path transform="translate({x + offset + dx:.1f} {y + offset + dy:.1f}) '
                f'scale({scale:.3f})" fill="{color_}" d="{d}"/>')

    if color == "tiktok":
        out.append(path(-1.2, -1.2, "#25F4EE"))
        out.append(path(1.2, 1.2, "#FE2C55"))
    out.append(path(0, 0, ink))
    return "".join(out)


def card(x, y, name, latin, foot, icon, color, t, coming=False):
    dash = ' stroke-dasharray="6 5"' if coming else ""
    fill = "none" if coming else t["card"]
    body = [f'<rect x="{x}" y="{y}" width="{CARD_W}" height="{CARD_H}" rx="14" fill="{fill}" stroke="{t["line"]}"{dash}/>']
    body.append(tile(x + 14, y + (CARD_H - TILE) / 2, icon, color, t))
    tx = x + 14 + TILE + 12
    size = 17 if len(name) <= 6 or not name.isascii() else 15
    body.append(f'<text x="{tx}" y="{y + 27}" fill="{t["text"]}" font-family="{SANS}" font-size="{size}" font-weight="700">{escape(name)}</text>')
    body.append(f'<text x="{tx}" y="{y + 46}" fill="{t["sub"]}" font-family="{SANS}" font-size="12.5">{escape(latin)}</text>')
    foot_color = t["coming"] if coming else t["mono"]
    body.append(f'<text x="{tx}" y="{y + 63}" fill="{foot_color}" font-family="{MONO}" font-size="11.5">{escape(foot)}</text>')
    return "".join(body)


def header(y, text, t, color=None):
    return (f'<text x="{PAD}" y="{y}" fill="{color or t["head"]}" font-family="{SANS}" '
            f'font-size="13" font-weight="700" letter-spacing="1.5">{escape(text)}</text>')


def grid(items, y, t, coming=False):
    parts = []
    for i, item in enumerate(items):
        col, row = i % COLS, i // COLS
        x = PAD + col * (CARD_W + GAP)
        yy = y + row * (CARD_H + GAP)
        parts.append(card(x, yy, *item, t, coming=coming))
    rows = (len(items) + COLS - 1) // COLS
    return "".join(parts), y + rows * (CARD_H + GAP) - GAP


def draw(t):
    parts, y = [], PAD + 8
    total = sum(len(items) for _, _, items in GROUPS)
    parts.append(header(y, f"已支持 · SUPPORTED · {total}", t, t["text"]))
    y += 30
    for zh, en, items in GROUPS:
        parts.append(header(y, f"{zh} · {en} · {len(items)}", t))
        y += 14
        cards = [(n, l, c, i, col) for c, n, l, i, col in items]
        block, y = grid(cards, y, t)
        parts.append(block)
        y += 34
    y += 6
    parts.append(header(y, f"正在开发 · IN PROGRESS · {len(COMING)}", t, t["coming"]))
    y += 14
    cards = [(n, l, "coming soon", i, col) for n, l, i, col in COMING]
    block, y = grid(cards, y, t, coming=True)
    parts.append(block)
    height = y + PAD
    defs = ('<defs><linearGradient id="ig" x1="0" y1="1" x2="1" y2="0">'
            '<stop offset="0" stop-color="#FEDA75"/><stop offset=".3" stop-color="#FA7E1E"/>'
            '<stop offset=".6" stop-color="#D62976"/><stop offset="1" stop-color="#4F5BD5"/>'
            '</linearGradient></defs>')
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" '
            f'viewBox="0 0 {WIDTH} {height}" role="img" aria-label="Supported platforms">{defs}'
            f'<rect width="{WIDTH}" height="{height}" rx="24" fill="{t["panel"]}"/>'
            + "".join(parts) + "</svg>\n")


def main():
    for name, theme in THEMES.items():
        (OUT / f"platforms-{name}.svg").write_text(draw(theme))
        print("wrote", OUT / f"platforms-{name}.svg")


if __name__ == "__main__":
    main()
