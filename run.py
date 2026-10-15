"""Source launcher, also the PyInstaller entry point."""
from pathlib import Path
import sys

if not getattr(sys,"frozen",False):
    sys.path.insert(0,str(Path(__file__).parent/"src"))

from vectorcraft_sci.app import main

if __name__=="__main__":main()
