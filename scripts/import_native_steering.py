# SPDX-License-Identifier: AGPL-3.0-or-later
"""Export original-byte steering and rotation controls from the retained client.

Requires the pinned research scripts/dependencies and audited EXE from the
AHOM-01 retention batch. No simulator calculation supplies an expected value.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys
from import_native_flight import flight_fields


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('research-scripts', 'exe', 'original-index', 'out'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--atan-table', type=Path)
    args = parser.parse_args()
    sys.path.insert(0, str(args.research_scripts.resolve()))
    import akarius_homing_native as native
    import audit_akarius_homing as audit
    from unicorn import UC_HOOK_CODE, x86_const as reg

    native.initialize(args.exe)
    controls = []
    math_controls = {}
    physical_controls = []
    active = None
    reciprocal_input = None
    stack = []

    def pointer(name):
        return native.u.reg_read(getattr(reg, 'UC_X86_REG_' + name))

    def words(address, count):
        return list(native.get(address, '<' + 'I' * count))

    def scalar(name):
        return pointer(name) & 0xffffffff

    def observe(engine, address, size, data):
        nonlocal active, reciprocal_input
        rva = address - native.p.base
        sp = pointer('RSP')
        if rva in (0x17b2e25, 0x17b2f04, 0x17b2f71):
            reciprocal_input = scalar('XMM2')
        elif rva in (0x17b2e28, 0x17b2f07, 0x17b2f74):
            row = {'kind': 'reciprocal_sqrt', 'inputs': [[reciprocal_input]],
                   'outputs': [[scalar('XMM1' if rva == 0x17b2f74 else 'XMM0')]]}
            key = json.dumps([row['kind'], row['inputs']], separators=(',', ':'))
            assert key not in math_controls or math_controls[key] == row
            math_controls[key] = row
            reciprocal_input = None
        if stack and stack[-1]['return'] == address:
            call = stack.pop()
            result = [[scalar(output)] if isinstance(output, str) else words(output, count)
                      for output, count in call['outputs']]
            key = json.dumps([call['kind'], call['inputs']], separators=(',', ':'))
            row = {'kind': call['kind'], 'inputs': call['inputs'], 'outputs': result}
            assert key not in math_controls or math_controls[key] == row
            math_controls[key] = row
        if rva == 0x16f990:
            assert active is None
            obj = pointer('RCX')
            active = {
                'obj': obj,
                'turning': words(obj + 0x171c, 1)[0],
                'has_target': bool(native.get(native.get(obj + 0xf38, '<Q')[0], '<Q')[0]),
                'dt': words(sp + 0x28, 1)[0],
                'velocity': words(obj + 0x1710, 3),
                'desired': None,
            }
        elif active is not None:
            if rva == 0x16f9cb:
                active['current'] = words(sp + 0x30, 3)
            elif rva == 0x16fa5d:
                active['desired'] = words(pointer('RAX'), 3)
            elif rva == 0x16fbbe:
                active['unwrapped'] = words(sp + 0x30, 3)
                active['next_velocity'] = words(active['obj'] + 0x1710, 3)
            elif rva == 0x16fbc8:
                active['wrapped'] = words(sp + 0x30, 3)
                del active['obj']
                controls.append(active)
                active = None
        kind, inputs, outputs = None, None, None
        if rva in (0x643e30, 0xea21c0):
            count = 4 if rva == 0x643e30 else 16
            kind = 'quaternion_to_euler' if rva == 0x643e30 else 'basis_to_euler'
            inputs, outputs = [words(pointer('RDX'), count)], [(pointer('RCX'), 3)]
        elif rva == 0x17f6720:
            kind, inputs, outputs = 'wrap', [words(pointer('RCX'), 3)], [(pointer('RCX'), 3)]
        elif rva == 0xea6930:
            kind, inputs, outputs = 'euler_to_basis', [words(pointer('R8'), 3)], [(pointer('RCX'), 16)]
        elif rva == 0x14f52a0:
            kind = 'euler_to_quaternion'
            inputs = [[scalar('XMM' + str(i)) for i in (1, 2, 3)]]
            outputs = [(pointer('RCX'), 4)]
        elif rva == 0x3a5fe0:
            kind, inputs = 'sin_cos', [words(pointer('R8'), 4)]
            outputs = [(pointer('RCX'), 4), (pointer('RDX'), 4)]
        elif rva == 0x298080:
            kind, inputs, outputs = 'quaternion_direction', [words(pointer('RDX'), 4)], [(pointer('RCX'), 4)]
        elif rva == 0x17b2dd0:
            kind = 'look_at'
            up = native.get(sp + 0x28, '<Q')[0]
            inputs = [words(pointer('R8'), 4), words(pointer('R9'), 4), words(up, 4)]
            outputs = [(pointer('RCX'), 16)]
        elif rva in (0x1c27110, 0x1c26ef0):
            kind = 'atan2' if rva == 0x1c27110 else 'asin'
            inputs = [[scalar('XMM' + str(i)) for i in range(2 if kind == 'atan2' else 1)]]
            outputs = [('XMM0', 1)]
        if kind is not None:
            stack.append({'kind': kind, 'inputs': inputs, 'outputs': outputs, 'return': native.get(sp, '<Q')[0]})

    original_call = native.call

    def call(*values, **options):
        result = original_call(*values, **options)
        observe(native.u, pointer('RIP'), 0, None)
        return result

    native.call = call
    native.u.hook_add(UC_HOOK_CODE, observe)
    native.u.reg_write(reg.UC_X86_REG_RBX, native.obj)
    native.u.emu_start(native.p.base + 0x48be4c, native.p.base + 0x48bf90, count=300)
    native.u.emu_start(native.p.base + 0x48c1c6, native.p.base + 0x48c1d0, count=10)
    original = json.loads(args.original_index.read_bytes())
    for name, run in (
        ('seek_cases', audit.seek_cases), ('motion_cases', audit.motion_cases),
        ('collision_gate_cases', audit.collision_gates), ('helix_input_cases', audit.helix_input_cases),
    ):
        assert json.dumps(run(), sort_keys=True) == json.dumps(original[name], sort_keys=True), name
    original_count = len(controls)
    for retained in (False, True):
        for euler in ((0., 0., 0.), (179., 80., 30.), (-179., -80., -30.)):
            for direction in ((1., 1., -1.), (-1., -1., 1.), (0., 0., 0.), (1e-6, 0., 0.)):
                for turning in (0.08, 0., 1e-7):
                    for dt in (0., 1 / 120, .5):
                        native.reset(locked=retained)
                        native.u.reg_write(reg.UC_X86_REG_RCX, native.obj + 0x1760)
                        for i, value in enumerate(euler, 1):
                            native.sf(getattr(reg, 'UC_X86_REG_XMM' + str(i)), value)
                        native.call(0x14f52a0)
                        native.put(native.obj + 0x171c, '<f', turning)
                        native.put(native.obj + 0x1710, '<3f', 37., -1900., 5.)
                        vector, output = native.mem + 0x1e0000, native.mem + 0x1e0100
                        native.put(vector, '<4f', *direction, 0.)
                        native.u.reg_write(reg.UC_X86_REG_RCX, native.obj)
                        native.u.reg_write(reg.UC_X86_REG_RDX, output)
                        native.u.reg_write(reg.UC_X86_REG_R8, native.obj + 0x1760)
                        native.u.reg_write(reg.UC_X86_REG_R9, vector)
                        native.put(native.stack + 0x28, '<f', dt)
                        native.call(0x16f990)
    assert active is None and not stack, (active, stack[-1:] if stack else [])
    for retained in (False, True):
        for euler in ((0., 0., 0.), (90., 40., 30.), (-90., -40., -30.), (179., 80., 60.), (-179., -80., -60.)):
            for point in ((-5., 3., -2.), (4., -1., 7.), (0., 1.5, .3)):
                for dt in (0., 1 / 120, 1 / 30, .5):
                    audit.fixture(position=(0., 1.5, .3), center=point, locked=retained)
                    native.u.reg_write(reg.UC_X86_REG_RCX, native.obj + 0x1760)
                    for i, value in enumerate(euler, 1):
                        native.sf(getattr(reg, 'UC_X86_REG_XMM' + str(i)), value)
                    native.call(0x14f52a0)
                    native.put(native.obj + 0x90, '<4f', *native.get(native.obj + 0x1760, '<4f'))
                    native.put(native.obj + 0x1710, '<3f', 37., -19., 5.)
                    native.put(native.obj + 0x1844, '<f', 20.)
                    before = flight_fields(native.u.mem_read, native.obj)
                    native.step(dt)
                    physical_controls.append({'seconds': struct.unpack('<I', struct.pack('<f', dt))[0],
                                              'before': before, 'after': flight_fields(native.u.mem_read, native.obj)})
    assert active is None and not stack
    digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    table = native.p.pe.get_data(0x241ee70, 241 * 8)
    if args.atan_table is not None:
        values = struct.unpack('<241Q', table)
        lines = ['// SPDX-License-Identifier: AGPL-3.0-or-later',
                 '// Native1c27130 table at PE RVA241ee70; imported by import_native_steering.py.',
                 'pub const ATAN: [u64; 241] = [']
        lines.extend('    ' + ', '.join(f'0x{word:016x}' for word in values[i:i+4]) + ',' for i in range(0, 241, 4))
        args.atan_table.write_text('\n'.join(lines) + '\n];\n')
    result = {
        'batch': '20261118T083810Z-native-3d-homing-replay-retention',
        'manifest_sha256': '5f57e9c7c377f16b7571b2900a6f7ab8f9c76d9fa584e316b0c16fb7eb72aea3',
        'executable_sha256': digest(args.exe),
        'importer_sha256': digest(Path(__file__)),
        'flight_importer_sha256': digest(Path(__file__).with_name('import_native_flight.py')),
        'original_index_sha256': digest(args.original_index),
        'research_scripts': {name: digest(args.research_scripts / name) for name in ('akarius_homing_native.py', 'audit_akarius_homing.py', 'native_evidence.py')},
        'scope': 'Original-byte numerical stages under supplied state; no live scene or producer installation',
        'atan_table': {'rva': '0x241ee70', 'bytes': len(table), 'sha256': hashlib.sha256(table).hexdigest()},
        'look_at_fallback_axes': [words(native.p.base + address, 4) for address in (0x28901b0, 0x28901d0, 0x28901c0)],
        'scalar_encoding': 'IEEE754 binary32 unsigned words, preserving signed zero',
        'original_controls': original_count,
        'steering': controls,
        'physical_controls': physical_controls,
        'rotation': list(math_controls.values()),
    }
    args.out.write_text(json.dumps(result, separators=(',', ':'), allow_nan=False) + '\n')
    print(json.dumps({'steering': len(controls), 'original_controls': original_count, 'rotation': len(math_controls), 'sha256': digest(args.out)}))


if __name__ == '__main__':
    main()
