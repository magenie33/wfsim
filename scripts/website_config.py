"""Render the HK entrance from the website's authoritative static routing rules."""
import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def files():
    source = (ROOT / 'deploy/website.nginx').read_text()
    maps_end = source.index('server {\n')
    locations_start = source.index('    location ~ ^/(api/|')
    locations_end = source.rfind('}\n')
    if not 0 < maps_end < locations_start < locations_end or source[locations_end:] != '}\n':
        raise ValueError('website routing boundaries changed; review the HK renderer')
    return {
        'wfsim-website-maps.conf': source[:maps_end],
        'wfsim-website-locations.conf': source[locations_start:locations_end],
        'wfsim-hk-website.conf': (ROOT / 'deploy/hk-website.nginx').read_text(),
        'wfsim-website-public.conf': (ROOT / 'deploy/website-public.conf').read_text(),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    for name, body in files().items():
        (args.output / name).write_text(body)


if __name__ == '__main__':
    main()
