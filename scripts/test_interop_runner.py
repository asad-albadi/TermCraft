import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('runner', Path(__file__).with_name('check-interop.py'))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)

class RunnerContract(unittest.TestCase):
    def test_zero_or_ignored_is_not_passing(self):
        for output in ['test result: ok. 0 passed; 0 failed; 0 ignored;', 'test result: ok. 0 passed; 0 failed; 1 ignored;']:
            self.assertEqual(runner.test_outcome(0, output), 'failed')
    def test_assertion_must_run_and_exit_successfully(self):
        self.assertEqual(runner.test_outcome(0, 'test result: ok. 1 passed; 0 failed; 0 ignored;'), 'passed')
        self.assertEqual(runner.test_outcome(1, 'test result: ok. 1 passed; 0 failed; 0 ignored;'), 'failed')
    def test_missing_fixture_is_unavailable(self):
        self.assertIn('ssh-keygen', runner.fixture_unavailability(['ssh-keygen']))
        self.assertIsNone(runner.fixture_unavailability([]))
    def test_every_cell_gets_record_even_when_unavailable(self):
        case = dict(id='native', endpoint='OpenSSH', version='unavailable', auth='key', trust='known', jump='direct', operation='shell', expected='SSH_OK', unavailable='Native Windows unavailable')
        record = runner.evidence_record('V01', case, 'unavailable', 'not executed', 'native.log', 'pending', '2026-11-25')
        self.assertEqual(record['result'], 'unavailable')
        self.assertTrue(record['remaining_limitations'])
        self.assertIn('Native Windows', record['remaining_limitations'][0])

if __name__ == '__main__': unittest.main()
