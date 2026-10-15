"""Exercise parameter-only native colour selection in an owned desktop session."""
import argparse
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
from vectorcraft_sci.protocol import ControlClient


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--port', type=int, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    c = ControlClient(args.port)
    checks = []

    def check(name, ok):
        if not ok:
            raise AssertionError(name)
        checks.append(name)
        print('PASS', name, flush=True)

    before = c.inspect()
    for target, hex_color in [('labels', '2391AF'), ('borders', 'D46B38')]:
        c.execute('ui.colorPicker', scientificTarget=target)
        c.call('ui.dialog.set', {'field': 'hex', 'value': hex_color})
        time.sleep(.15)
        check(target + ' preview leaves entire document unchanged', c.inspect() == before)
        c.call('ui.screenshot', {'path': str((args.output / (target + '-picker.png')).resolve())})
        c.call('ui.dialog.confirm')
        check(target + ' confirm leaves entire document unchanged', c.inspect() == before)
        settings = c.execute('ui.scientific', operation='inspect')['settings']
        field = 'label_color' if target == 'labels' else 'border_color'
        check(target + ' parameter receives chosen color', settings[field] == '#' + hex_color)
    settings = c.execute('ui.scientific', operation='inspect')
    c.execute('ui.colorPicker', scientificTarget='labels')
    c.call('ui.dialog.set', {'field': 'hex', 'value': 'FF0000'})
    c.call('ui.dialog.cancel')
    check('cancel retains parameters and recent colors', c.execute('ui.scientific', operation='inspect') == settings)
    check('cancel retains document and undo history', c.inspect() == before)
    check('text and border colors remain separate', settings['settings']['label_color'] != settings['settings']['border_color'])
    check('native swatch query changes no document', bool(c.execute('swatch.list')['swatches']) and c.inspect() == before)
    c.execute('ui.scientific', operation='preset_save', settings={'preset_name':'颜色验证'})
    c.execute('ui.scientific', operation='set_color', target='labels', color='#ABC')
    c.execute('ui.scientific', operation='preset_load', settings={'preset_name':'颜色验证'})
    check('preset restores independent colors', c.execute('ui.scientific', operation='inspect')['settings']['label_color'] == '#2391AF')
    check('parameter changes consume no undo', c.inspect() == before)
    (args.output / 'palette-report.json').write_text(json.dumps({'checks':checks,'passed':len(checks)},ensure_ascii=False,indent=2),encoding='utf-8')


if __name__ == '__main__':
    main()
