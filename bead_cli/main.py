# PYTHON_ARGCOMPLETE_OK
from collections.abc import Sequence
import importlib.metadata
import os
import subprocess
import sys
import textwrap
import traceback

import appdirs

from bead.tech.fs import Path
from bead.tech.timestamp import timestamp

from . import box
from . import input
from . import workspace
from .cmdparse import Command
from .cmdparse import Parser
from .environment import Environment
from .web import commands as web


def output_of(shell_cmd: str):
    return subprocess.check_output(shell_cmd, shell=True).decode('utf-8').strip()


def get_version_info():
    try:
        version = importlib.metadata.version('bead')
    except importlib.metadata.PackageNotFoundError:
        version = 'unknown'

    return textwrap.dedent(
        f'''
        Python:
        ------
        {sys.version}

        Bead:
        ----
        {version}
        '''
    )


class CmdVersion(Command):
    '''
    Show program version info
    '''

    def run(self, args, env: 'Environment'):
        print(get_version_info())


def make_argument_parser(defaults):
    parser = Parser.new(defaults)
    (parser
        .commands(
            ('new', workspace.CmdNew, 'Create and initialize new workspace directory with a new bead.'),
            ('develop', workspace.CmdDevelop, 'Create workspace from specified bead.'),
            ('save', workspace.CmdSave, 'Save workspace in a box.'),
            ('status', workspace.CmdStatus, 'Show workspace information.'),
            ('web', web.CmdWeb, 'Manage/visualize the big picture - connections between beads.'),
            ('zap', workspace.CmdZap, 'Delete workspace.'),
            ('version', CmdVersion, 'Show program version.'),
        ))

    (parser
        .group('input', 'Manage data loaded from other beads')
        .commands(
            ('add', input.CmdAdd, 'Define dependency and load its data.'),
            ('delete', input.CmdDelete, 'Forget all about an input.'),
            ('rm', input.CmdDelete, 'Forget all about an input. (alias for delete)'),
            ('update', input.CmdUpdate, 'Update input[s] to newest version or defined bead.'),
            ('load', input.CmdLoad, 'Load data from already defined dependency.'),
            ('unload', input.CmdUnload, 'Unload input data.'),
        ))

    (parser
        .group('box', 'Manage bead boxes')
        .commands(
            ('add', box.CmdAdd, 'Define a box.'),
            ('list', box.CmdList, 'Show known boxes.'),
            ('forget', box.CmdForget, 'Forget a known box.'),
        ))

    parser.autocomplete()

    return parser


def run(config_dir: str, argv: Sequence[str]):
    parser_defaults = dict(config_dir=Path(config_dir))
    parser = make_argument_parser(parser_defaults)

    # Create config directory if it doesn't exist
    config_path = Path(config_dir)
    try:
        os.makedirs(config_path)
    except OSError:
        if not os.path.isdir(config_path):
            raise

    env = Environment.from_dir(config_path)
    return parser.dispatch(argv, env)


FAILURE_TEMPLATE = """\
{exception}

If you are using the latest version, and have not reported this error yet
please report this problem by copy-pasting the content of file {error_report}
at {repo}/issues/new
and/or attaching the file to an email to {dev}@gmail.com.

Please make sure you copy-paste from the file {error_report}
and not from the console, as the shown exception text was shortened
for your convenience, thus it is not really helpful in fixing the bug.
"""


def main(run=run):
    config_dir = appdirs.user_config_dir(
        'bead_cli-6a4d9d98-8e64-4a2a-b6c2-8a753ea61daf')
    try:
        retval = run(config_dir, sys.argv[1:])
    except KeyboardInterrupt:
        print("Interrupted :(", file=sys.stderr)
        retval = -1
    except SystemExit:
        raise
    except BaseException:
        # all remaining erroros are catched - including RunTimeErrors
        sys_argv = f'{sys.argv!r}'
        exception = traceback.format_exc()
        short_exception = traceback.format_exc(limit=1)
        error_report = os.path.realpath(f'error_{timestamp()}.txt')
        with open(error_report, 'w') as f:
            f.write(f'sys_argv = {sys_argv}\n')
            f.write(f'{exception}\n')
            f.write(f'{get_version_info()}\n')
        print(
            FAILURE_TEMPLATE.format(
                exception=short_exception,
                error_report=error_report,
                repo='https://github.com/e3krisztian/bead',
                dev='e3krisztian',
            ),
            file=sys.stderr
        )
        retval = -1
    sys.exit(retval)


if __name__ == '__main__':
    main()
