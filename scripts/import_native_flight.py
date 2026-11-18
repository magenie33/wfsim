# SPDX-License-Identifier: AGPL-3.0-or-later
"""Extract numerical flight controls from retained native call-state pages."""
import argparse
import hashlib
import json
from pathlib import Path
import struct


def flight_fields(read, obj):
    def number(address, fmt):
        return struct.unpack(fmt, read(address, struct.calcsize(fmt)))[0]

    result = {}
    for name, offset, count in (
        ('position', 0x1750, 3), ('world_position', 0x70, 3),
        ('orientation', 0x1760, 4), ('world_orientation', 0x90, 4),
        ('velocity', 0xaf0, 3), ('angular_velocity', 0x1710, 3),
        ('target', 0x1740, 3),
    ):
        result[name] = list(struct.unpack('<' + 'I' * count, read(obj + offset, 4 * count)))
    for name, offset in (('bank', 0x1844), ('flight', 0x1d1c), ('turning', 0x171c),
                         ('acceleration', 0xab0), ('maximum', 0xac8), ('steering_seconds', 0x1720)):
        result[name] = number(obj + offset, '<I')
    result['retained'] = number(number(obj + 0xf38, '<Q'), '<Q') != 0
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('replay', 'exe', 'out'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    digest = lambda raw: hashlib.sha256(raw).hexdigest()
    executable = args.exe.read_bytes()
    assert digest(executable) == 'dae100202115919c89440c9ef320f691113ee8dcaf01205cbd180072c35aabdb'
    pe = struct.unpack_from('<I', executable, 0x3c)[0]
    optional = pe + 24
    base = struct.unpack_from('<Q', executable, optional + 24)[0]
    image = bytearray(struct.unpack_from('<I', executable, optional + 56)[0])
    first = optional + struct.unpack_from('<H', executable, pe + 20)[0]
    for index in range(struct.unpack_from('<H', executable, pe + 6)[0]):
        address, size, offset = struct.unpack_from('<III', executable, first + 40 * index + 12)
        image[address:address + size] = executable[offset:offset + size]
    calls_raw = (args.replay / 'calls.json').read_bytes()
    assert digest(calls_raw) == '518d311c8c92f27c6a81ec7f5dcde0f5ca71bf640e62c609451ec91c85772204'
    cache = {}

    def read(state, address, count):
        result = bytearray()
        while count:
            page = address & ~4095
            offset = address - page
            size = min(count, 4096 - offset)
            key = state['fixture_nonzero_pages'].get(hex(page)) or state['image_page_overrides'].get(hex(page))
            if key is not None:
                if key not in cache:
                    raw = (args.replay / 'pages' / (key + '.bin')).read_bytes()
                    assert len(raw) == 4096 and digest(raw) == key
                    cache[key] = raw
                result.extend(cache[key][offset:offset + size])
            elif base <= address < base + len(image):
                result.extend(image[address - base:address - base + size])
            else:
                result.extend(bytes(size))
            address += size
            count -= size
        return result

    def number(state, address, fmt):
        return struct.unpack(fmt, read(state, address, struct.calcsize(fmt)))[0]

    cases = []
    sequence = -1
    for call in json.loads(calls_raw):
        if int(call['begin'], 16) != base + 0x11e65e0:
            continue
        before, after = call['before'], call['after']
        obj = int(before['registers_hex']['RCX'], 16)
        initial = flight_fields(lambda address, count: read(before, address, count), obj)
        assert number(before, obj + 0x1d48, '<B') == 0
        assert number(before, obj + 0xb30, '<B') == 0
        assert number(before, base + 0x295461c, '<B') == 0
        assert number(before, number(before, obj + 0x1a10, '<Q'), '<Q') == 0
        if initial['flight'] == 0:
            sequence += 1
        cases.append({'group': call['group'], 'sequence': sequence,
                      'seconds': int(before['registers_hex']['XMM1'], 16) & 0xffffffff,
                      'before': initial, 'after': flight_fields(lambda address, count: read(after, address, count), obj)})
    result = {
        'batch': '20261118T083810Z-native-3d-homing-replay-retention',
        'manifest_sha256': '5f57e9c7c377f16b7571b2900a6f7ab8f9c76d9fa584e316b0c16fb7eb72aea3',
        'executable_sha256': digest(executable), 'calls_sha256': digest(calls_raw),
        'importer_sha256': digest(Path(__file__).read_bytes()),
        'scope': 'Native11e65e0 default numerical route under supplied target/clock/world transforms and empty-world movement; no collision certification',
        'qualification': {'state_1d48': 0, 'gravity_b30': 0, 'global_295461c': 0,
                          'reference_1a10_empty': True, 'world_transform_supplied': True},
        'scalar_encoding': 'IEEE754 binary32 unsigned words',
        'cases': cases,
    }
    args.out.write_text(json.dumps(result, separators=(',', ':')) + '\n')
    print(json.dumps({'steps': len(cases), 'sequences': sequence + 1, 'consumed_pages_verified': len(cache), 'sha256': digest(args.out.read_bytes())}))


if __name__ == '__main__':
    main()
