import argparse
import os
import subprocess
import textwrap
from typing import TYPE_CHECKING
from typing import Set
import webbrowser

from bead import tech
from bead.box import search_boxes

from . import sketch as web_sketch
from ..cmdparse import Command
from ..common import die
from .dummy import Dummy
from .io import read_beads
from .io import write_beads
from .sketch import Sketch

if TYPE_CHECKING:
    from ..environment import Environment


class CmdWeb(Command):
    '''
    Visualize the big picture.

    Capture/load/filter/save/visualize connections between all available
    computation archives.

    Sub-commands describe a processing pipe-line, where each sub command
    work on an input graph, and yield an output graph.

    The processing pipe-line by default starts off with the graph of
    available archives and their input connections clustered by name.
    (see also "load" below for an alternative, speedier initial graph)

    Available pipe-line commands:

    load filename.web
        Throw away current graph and load previously exported web from file.
        When it is the first command, discovering all archives is skipped.

    / [source-name[s]] .. [sink-name[s]] /
        Filters computations by following input connections:
        - if the set of sources is not empty:
          drop all that do not have any of the sources as direct/indirect
          inputs.
        - if the set of sinks is not empty:
          drop all that are not direct/indirect inputs to any of the sinks.

    save filename.web
        Save current web metadata to file - ("load" above is one use case).

    png filename.png
        Save connections as image in PNG format

    svg filename.svg
        Save connections as image in SVG format

    color
        Assign freshness to nodes, which are visualized as colors.
        Answers the question: "Are all input at the latest version?"

    heads
        Reduce graph to include only most recent computations per
        cluster and possibly a few older ones, that are referenced
        by outdated, but not yet superseded (updated) computations.

    view filename
        open filename in browser (shortcut after save/png/svg)
    '''

    FORMATTER_CLASS = argparse.RawDescriptionHelpFormatter

    def declare(self, arg):
        arg(
            'words',
            metavar='...',
            nargs=argparse.REMAINDER,
            help='Sub-commands and their arguments'
        )

    def run(self, args, env: 'Environment'):
        if not args.words:
            print('No sub-commands given, see usage below:')
            print(textwrap.dedent(self.__doc__))
            return

        commands, remaining_words = parse_commands(env, args.words)
        if remaining_words:
            msg = 'Could not fully parse command line.\n'
            if commands:
                msg += 'Parsed commands:'
                msg += '\n\t'.join(map(str, commands))
            msg += f'\nCould not parse: {remaining_words}'
            die(msg)

        sketch = Sketch.from_beads([])
        for command in commands:
            sketch = command(sketch)


def parse_commands(env, words):
    remaining_words = words[::-1]
    commands = []

    if remaining_words and remaining_words[-1] != 'load':
        commands.append(LoadAll(env.get_boxes()))

    while remaining_words:
        remaining = remaining_words[:]
        cmd_name = remaining_words.pop()
        try:
            cmd_class = SUBCOMMANDS[cmd_name]
            cmd = cmd_class(remaining_words)
        except:
            return commands, remaining[::-1]
        commands.append(cmd)

    return commands, remaining_words[::-1]


class SketchProcessor:
    def __init__(self, _args):
        pass

    def __call__(self, sketch):
        return sketch

    def __str__(self):
        cls = self.__class__.__name__
        args = vars(self)
        return f'{cls}({args})'

    def sketch_from_beads(self, beads):
        return Sketch.from_beads([Dummy.from_bead(bead) for bead in beads])


class ProcessorWithFileName(SketchProcessor):
    def __init__(self, args):
        self.file_name = tech.fs.Path(args.pop())


class LoadAll(SketchProcessor):
    def __init__(self, boxes):
        super().__init__([])
        self.boxes = boxes

    def __call__(self, _sketch):
        beads = load_all_beads(self.boxes)
        print(f"Loaded {len(beads)} beads")
        return self.sketch_from_beads(beads)


class Load(ProcessorWithFileName):
    def __call__(self, _sketch):
        beads = read_beads(self.file_name)
        return self.sketch_from_beads(beads)


class Save(ProcessorWithFileName):
    def __call__(self, sketch):
        write_beads(self.file_name, sketch.beads)
        return sketch


class WriteDot(ProcessorWithFileName):
    def __call__(self, sketch):
        dot_str = sketch.as_dot()
        tech.fs.write_file(self.file_name, dot_str)
        return sketch


class WritePng(ProcessorWithFileName):
    def __call__(self, sketch):
        dot_str = sketch.as_dot()
        print(f"Creating PNG: {self.file_name}")
        graphviz_dot(dot_str, self.file_name, format='png')
        return sketch


class WriteSvg(ProcessorWithFileName):
    def __call__(self, sketch):
        dot_str = sketch.as_dot()
        print(f"Creating SVG: {self.file_name}")
        graphviz_dot(dot_str, self.file_name, format='svg')
        return sketch


class View(ProcessorWithFileName):
    def __call__(self, sketch):
        print(f"Viewing {self.file_name}")
        webbrowser.open(self.file_name.as_posix())


class Filter(SketchProcessor):
    def __init__(self, args):
        self.sources = self._pop_names(args, sentinel='..')
        self.sinks = self._pop_names(args, sentinel='/')
        super().__init__(args)

    def _pop_names(self, args, sentinel) -> Set[str]:
        names: Set[str] = set()
        while args:
            name = args.pop()
            if name == sentinel:
                return names
            if name in ('..', '/'):
                raise ValueError(f'Unexpected delimiter: {repr(name)} after {names}.')
            if not is_valid_name(name):
                raise ValueError(f'Malformed name: {repr(name)} after {names}.')
            names.add(name)
        raise ValueError(f'Delimiter not found: {repr(sentinel)}.')

    def __call__(self, sketch):
        if self.sources:
            sketch = web_sketch.set_sources(sketch, self.sources)
        if self.sinks:
            sketch = web_sketch.set_sinks(sketch, self.sinks)
        return sketch


def is_valid_name(name):
    return name not in ('..', '/')


class SetFreshness(SketchProcessor):
    def __call__(self, sketch):
        sketch.color_beads()
        return sketch


class KeepOnlyHeads(SketchProcessor):
    def __call__(self, sketch):
        return web_sketch.heads_of(sketch).drop_deleted_inputs()


SUBCOMMANDS = {
    'load': Load,
    'save': Save,
    'dot': WriteDot,
    'png': WritePng,
    'svg': WriteSvg,
    '/': Filter,
    'color': SetFreshness,
    'heads': KeepOnlyHeads,
    'view': View,
}


def load_all_beads(boxes):
    columns = int(os.environ.get('COLUMNS', 80))
    all_beads = []
    import time
    load_start = time.perf_counter()
    # This search_boxes().all() is the meat, the rest is just user feedback for big/slow
    # environments
    for n, bead in enumerate(search_boxes(boxes).all()):
        load_end = time.perf_counter()

        msg = f"\rLoaded bead {n + 1} ({bead.location})"[:columns]
        msg = msg + ' ' * (columns - len(msg))
        print(msg, end="", flush=True)
        if load_end - load_start > 1:
            print(f"\nLoading took {load_end - load_start} seconds")
        all_beads.append(bead)
        load_start = time.perf_counter()
    print("\r" + " " * columns + "\r", end="")
    return all_beads


def graphviz_dot(dot_str, output_file, format):
    cmd = ['dot', '-o', output_file, '-T', format]
    subprocess.run(cmd, input=dot_str.encode('utf-8'), capture_output=True, check=True)
