#!/usr/bin/env python3
"""Compose the README artwork from unmodified app captures; requires rsvg-convert."""

import base64
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def embedded(path):
    return "data:image/png;base64," + base64.b64encode((ROOT / path).read_bytes()).decode("ascii")


def render():
    logo = embedded("assets/termcraft-logo.png")
    dark = embedded("shots/full-hd/terminal-dark.png")
    light = embedded("shots/full-hd/workspace-light.png")
    svg = f'''<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="1920" height="1080" viewBox="0 0 1920 1080" role="img" aria-labelledby="title description">
  <title id="title">TermCraft — one workspace for your connections</title>
  <desc id="description">Emerald presentation artwork with real Linux captures of the dark terminal and light session workspace. Linux, Windows and macOS are supported targets.</desc>
  <defs>
    <linearGradient id="background" x2="1" y2="1"><stop stop-color="#082b25"/><stop offset="1" stop-color="#061712"/></linearGradient>
    <radialGradient id="glow"><stop stop-color="#00b88c" stop-opacity=".18"/><stop offset="1" stop-color="#00b88c" stop-opacity="0"/></radialGradient>
    <filter id="shadow" x="-15%" y="-20%" width="140%" height="150%"><feDropShadow dx="0" dy="18" stdDeviation="20" flood-color="#000" flood-opacity=".4"/></filter>
    <clipPath id="dark-clip"><rect x="80" y="390" width="1200" height="675" rx="12"/></clipPath>
    <clipPath id="light-clip"><rect x="990" y="370" width="850" height="478.125" rx="12"/></clipPath>
  </defs>
  <rect width="1920" height="1080" fill="url(#background)"/>
  <ellipse cx="1300" cy="420" rx="850" ry="660" fill="url(#glow)"/>
  <path d="M0 358H1920" stroke="#b5ffdf" stroke-opacity=".09"/>
  <g font-family="DejaVu Sans, sans-serif">
    <image x="80" y="60" width="72" height="72" xlink:href="{logo}"/>
    <text x="174" y="108" fill="#f2fff9" font-size="44" font-weight="bold" letter-spacing="-1.5">TermCraft</text>
    <text x="80" y="228" fill="#f2fff9" font-size="66" font-weight="bold" letter-spacing="-2.7">One workspace for your connections.</text>
    <text x="84" y="285" fill="#a6c7bc" font-size="24">Local terminals, remote sessions and everyday tools. Together.</text>
    <g font-size="19" fill="#d4f5e7" text-anchor="middle">
      <rect x="1390" y="77" width="126" height="44" rx="22" fill="#123d31" stroke="#356353"/><text x="1453" y="105">Linux</text>
      <rect x="1530" y="77" width="150" height="44" rx="22" fill="#123d31" stroke="#356353"/><text x="1605" y="105">Windows</text>
      <rect x="1694" y="77" width="146" height="44" rx="22" fill="#123d31" stroke="#356353"/><text x="1767" y="105">macOS</text>
    </g>
    <text x="1010" y="350" fill="#b5d6c8" font-size="17" letter-spacing="2">LIGHT WORKSPACE</text>
    <rect x="989" y="369" width="852" height="480.125" rx="13" fill="#a6d3c0" filter="url(#shadow)"/>
    <image x="990" y="370" width="850" height="478.125" clip-path="url(#light-clip)" xlink:href="{light}"/>
    <text x="80" y="367" fill="#b5d6c8" font-size="17" letter-spacing="2">DARK TERMINAL</text>
    <rect x="79" y="389" width="1202" height="677" rx="13" fill="#42775f" filter="url(#shadow)"/>
    <image x="80" y="390" width="1200" height="675" clip-path="url(#dark-clip)" xlink:href="{dark}"/>
    <text x="1340" y="919" fill="#e3fff1" font-size="28" font-weight="bold">Open source.</text>
    <text x="1340" y="960" fill="#e3fff1" font-size="28" font-weight="bold">Local. Yours.</text>
    <text x="1340" y="1017" fill="#91b8a7" font-size="16">Actual Linux app captures · demo workspace</text>
    <text x="1340" y="1045" fill="#91b8a7" font-size="16">github.com/asad-albadi/TermCraft</text>
  </g>
</svg>'''
    output = ROOT / "shots/termcraft-mockup.svg"
    output.write_text(svg, encoding="utf-8")
    subprocess.run(["rsvg-convert", str(output), "-o", str(output.with_suffix(".png"))], check=True)


if __name__ == "__main__":
    render()
