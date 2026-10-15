import json
from pathlib import Path
import re
import tempfile
import threading
import unittest
import urllib.request
import urllib.error

from vectorcraft_sci.app import Companion,make_server


class ServerTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.app=Companion(1,Path(self.temp.name))
        self.server=make_server(self.app)
        threading.Thread(target=self.server.serve_forever,daemon=True).start()
        self.base=f'http://127.0.0.1:{self.server.server_port}'
        self.html=urllib.request.urlopen(self.base).read().decode()
        self.token=re.search('name="session-token" content="([^"]+)"',self.html)[1]

    def tearDown(self):
        self.server.shutdown();self.server.server_close();self.temp.cleanup()

    def test_no_token_cannot_read_state_or_edit(self):
        for path in ['/api/state','/api/action']:
            with self.assertRaises(urllib.error.HTTPError) as err:
                urllib.request.urlopen(self.base+path)
            self.assertEqual(err.exception.code,403)

    def test_rebinding_and_cross_origin_refused(self):
        for headers in [{'Host':'evil.example'}, {'Origin':'https://evil.example','X-Toolbox-Token':self.token}]:
            req=urllib.request.Request(self.base+'/api/settings',data=b'{}',headers=headers)
            with self.assertRaises(urllib.error.HTTPError) as err:urllib.request.urlopen(req)
            self.assertEqual(err.exception.code,403)

    def test_disconnected_state_is_not_headless_success(self):
        req=urllib.request.Request(self.base+'/api/state',headers={'X-Toolbox-Token':self.token})
        result=json.load(urllib.request.urlopen(req))
        self.assertFalse(result['state']['connected'])

    def test_presets_round_trip_without_document_data(self):
        payload={'presets':{'三列':{'grid-form':{'columns':'3','keep_ratio':True}}}}
        req=urllib.request.Request(self.base+'/api/settings',data=json.dumps(payload).encode(),headers={'X-Toolbox-Token':self.token})
        urllib.request.urlopen(req).close()
        loaded=Companion(1,Path(self.temp.name)).settings
        self.assertEqual(loaded['presets'],payload['presets'])

    def test_no_arbitrary_file_serving(self):
        req=urllib.request.Request(self.base+'/../../pyproject.toml',headers={'X-Toolbox-Token':self.token})
        with self.assertRaises(urllib.error.HTTPError) as err:urllib.request.urlopen(req)
        self.assertEqual(err.exception.code,404)


if __name__=='__main__':unittest.main()
