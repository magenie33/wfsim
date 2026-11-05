#!/usr/bin/env python3
"""Exercise Wiki rank-table parsing and localized catalog generation."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yaml

from de_i18n import write_descriptions
from survey_mod_coverage import rank_metadata


class ModCoverage(unittest.TestCase):
    def metadata(self, table):
        with patch('survey_mod_coverage.fetch', return_value='==Stats==\n' + table):
            return rank_metadata({'Name': 'Example', 'BaseDrain': 99, 'MaxRank': 99})

    def test_flawed_table_does_not_replace_the_regular_rank_ladder(self):
        regular = '{|\n|-\n|0||+15%||2\n|-\n|1||+30%||3\n|}\n'
        flawed = '{|\n|-\n|0||+5%||0\n|}\n'
        self.assertEqual(self.metadata(regular + flawed), (2, 1))

    def test_a_rank_cell_on_its_own_line_is_not_a_distance(self):
        self.assertEqual(self.metadata('{|\n|-\n|0\n|25||50||0\n|-\n|1\n|25||100||1\n|}'), (0, 1))

    def test_a_partial_ladder_is_refused(self):
        with self.assertRaises(ValueError):
            self.metadata('{|\n|-\n|0||20%||2\n|-\n|2||60%||4\n|}')

    def test_a_manual_overlay_keeps_its_card_and_available_de_text_wins(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'descriptions.yaml'
            path.write_text('mod_descriptions:\n  known: [old]\n  unavailable: [retained]\n')
            (path.parent / 'manual.yaml').write_text('mod_descriptions:\n  manual: [owned]\n')
            write_descriptions(path, 'zh', {'mod_descriptions': {'known': ['new'], 'manual': ['duplicate'], 'added': ['fresh']}})
            self.assertEqual(yaml.safe_load(path.read_text())['mod_descriptions'],
                             {'known': ['new'], 'unavailable': ['retained'], 'added': ['fresh']})


if __name__ == '__main__':
    unittest.main()
