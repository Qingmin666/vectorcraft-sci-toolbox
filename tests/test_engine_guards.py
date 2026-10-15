import unittest
from vectorcraft_sci.engine import Toolbox
from vectorcraft_sci.layout import LayoutError


class ReadOnlyFixture:
    """Small recorded-shape fixture; an edit reaching it is an error."""
    def ui(self):return {'activeDocument':0}
    def inspect(self):return {'title':'fixture','revision':8,'selection':[2],'layers':[{
        'id':1,'kind':'Layer','children':[{'id':2,'kind':'Image','name':'panel',
        'bounds':{'x':10.,'y':20.,'width':100.,'height':80.}}]}]}
    def execute(self,*a,**kw):raise AssertionError('No edit should be sent')


class GuardTests(unittest.TestCase):
    def test_noop_resize_does_not_consume_previous_undo(self):
        result=Toolbox(ReadOnlyFixture()).run('resize',{'width':100})
        self.assertIn('未产生编辑',result['message'])

    def test_stale_revision_rejected_before_edit(self):
        with self.assertRaises(LayoutError):
            Toolbox(ReadOnlyFixture()).run('resize',{'width':50},expected_revision=7)

    def test_stale_selection_rejected_before_edit(self):
        with self.assertRaises(LayoutError):
            Toolbox(ReadOnlyFixture()).run('resize',{'width':50},expected_selection=[3])

    def test_unknown_action_not_forwarded_as_engine_command(self):
        with self.assertRaises(LayoutError):
            Toolbox(ReadOnlyFixture()).run('file.deleteEverything')


if __name__=='__main__':unittest.main()
