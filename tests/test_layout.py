import math
import unittest
from vectorcraft_sci.layout import Box, LayoutError, grid_plan, resize_plan, align_plan, spacing_plan, matrix, label_text


class LayoutTests(unittest.TestCase):
    def setUp(self):
        self.boxes=[Box(1,10,20,100,80),Box(2,200,22,60,40),Box(3,10,200,120,50)]

    def test_grid_uses_largest_column_and_row(self):
        out=grid_plan(self.boxes,2,15,10)
        self.assertEqual([(b.x,b.y)for b in out],[(10,20),(145,20),(10,110)])

    def test_proportional_resize_each_individually(self):
        out=resize_plan(self.boxes,width=50)
        self.assertEqual(out[0].height,40)
        self.assertAlmostEqual(out[1].height,100/3)
        with self.assertRaises(LayoutError):resize_plan(self.boxes,50,50,True)

    def test_transform_maps_all_corners(self):
        a=Box(1,-10,20,30,50);b=Box(1,40,-20,90,100)
        m=matrix(a,b)
        self.assertEqual([m[0]*a.x+m[4],m[3]*a.y+m[5]],[b.x,b.y])
        self.assertEqual([m[0]*a.right+m[4],m[3]*a.bottom+m[5]],[b.right,b.bottom])

    def test_spacing_is_gap_not_centres(self):
        out=spacing_plan(self.boxes,'horizontal',7)
        for left,right in zip(out,out[1:]):self.assertAlmostEqual(right.x-left.right,7)

    def test_align_to_first_keeps_reference(self):
        out=align_plan(self.boxes,'right','first')
        self.assertEqual(out[0],self.boxes[0])
        self.assertTrue(all(b.right==110 for b in out))

    def test_labels_roll_over_after_z(self):
        self.assertEqual(label_text(26,'A'),'Z')
        self.assertEqual(label_text(27,'a)'),'aa)')

    def test_invalid_layout_does_not_produce_nan(self):
        for value in [0,-1,float('nan'),float('inf'),'oops',1.5]:
            with self.subTest(value=value):
                with self.assertRaises(LayoutError):grid_plan(self.boxes,value)
        with self.assertRaises(LayoutError):resize_plan([Box(1,0,0,0,5)],width=3)


if __name__=='__main__':unittest.main()
