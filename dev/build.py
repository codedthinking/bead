#!/usr/bin/env python3
# coding: utf-8

import contextlib
from glob import glob
import os
import shutil
import stat
from subprocess import run
from zipfile import ZIP_DEFLATED
from zipfile import ZipFile

BUILD = 'executables'
PKGS = BUILD + '/pkgs'
SRC = BUILD + '/src'
TOOL_PYZ = BUILD + '/bead.pyz'
UNIX_TOOL = BUILD + '/bead'
WIN_TOOL = BUILD + '/bead.cmd'


def mkdir(dir):
    if not os.path.isdir(dir):
        print(f'mkdir {dir}')
        os.makedirs(dir)


def pip(*args):
    print(f'pip {" ".join(args)}')
    return run(('pip',) + args, check=True)


def pip_download_source(*args):
    return pip('download', '--no-binary', ':all:', *args)


def rmtree(dir):
    print(f'rm -rf {dir}')
    shutil.rmtree(dir, ignore_errors=True)


def make_executable(file):
    print(f'chmod +x {file}')
    st = os.stat(file)
    os.chmod(file, st.st_mode | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)


@contextlib.contextmanager
def notification(msg, long_output=False):
    if long_output:
        print(msg + ':')
        print()
        print('-' * 32)
    else:
        print(' * ' + msg)
    try:
        yield
    finally:
        if long_output:
            print('-' * 32)
            print()


def further_output(msg):
    return notification(msg, long_output=True)


progress = notification

# start with no build directory
with further_output('Clean up'):
    rmtree(BUILD)

with further_output('Downloading dependencies'):
    mkdir(PKGS)
    pip_download_source('--dest', PKGS, '--exists-action', 'w', '-r', 'requirements.txt')

with further_output('Unpacking packages'):
    mkdir(SRC)
    for package in glob(PKGS + '/*'):
        pip('install', '--target', SRC, '--no-compile', '--no-deps', package)

with further_output('Building wheel with uv'):
    wheel_dir = BUILD + '/wheel'
    mkdir(wheel_dir)
    run(['uv', 'build', '--wheel', '--out-dir', wheel_dir],
        cwd='.', check=True)

    # Find the built wheel
    wheel_files = glob(wheel_dir + '/*.whl')
    if not wheel_files:
        raise RuntimeError("No wheel file found after uv build")
    [wheel_path] = wheel_files  # Should only be one wheel

with further_output('Installing our own package from wheel'):
    pip('install', '--target', SRC, '--no-compile', '--no-deps', wheel_path)

with progress(f'Creating .pyz zip archive from the sources ({TOOL_PYZ})'):
    with ZipFile(TOOL_PYZ, mode='w', compression=ZIP_DEFLATED) as zip:
        zip.write('__main__.py', '__main__.py')
        for realroot, dirs, files in os.walk(SRC):
            ziproot = os.path.relpath(realroot, SRC)
            for file_name in files:
                zip.write(
                    os.path.join(realroot, file_name),
                    os.path.join(ziproot, file_name))


def make_tool(tool_file_name, runner):
    with open(tool_file_name, 'wb') as f:
        f.write(runner)
        with open(TOOL_PYZ, 'rb') as pyz:
            f.write(pyz.read())


with progress(f'Creating unix tool ({UNIX_TOOL})'):
    UNIX_RUNNER = b'#!/usr/bin/env python3\n# PYTHON_ARGCOMPLETE_OK\n'

    make_tool(UNIX_TOOL, UNIX_RUNNER)
    make_executable(UNIX_TOOL)

with progress(f'Creating windows tool ({WIN_TOOL})'):
    WINDOWS_RUNNER = b'\r\n'.join((
        b'@echo off',
        b'python3.exe "%~f0" %*',
        b'exit /b %errorlevel%',
        b''))

    make_tool(WIN_TOOL, WINDOWS_RUNNER)

print('Done.')
