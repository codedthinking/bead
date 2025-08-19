'''
We are responsible to store (and retrieve) beads.

We are a convenience feature, as beads can be stored and used directly as files.

Boxes can be used to:
- share computations (beads) (when the box is on a shared drive (e.g. NFS or sshfs mounted))
- store separate computation branches (e.g. versions, that are released to the public)
- hide sensitive computations by splitting up storage according to access level
  (this is naive access control, but could work)
'''

from abc import ABC
from abc import abstractmethod
from dataclasses import dataclass
from enum import Enum
from enum import auto
from typing import Iterable
from typing import Iterator

from . import tech
from .bead import Archive
from .exceptions import BoxError
from .exceptions import InvalidArchive
from .tech.timestamp import time_from_timestamp
from .ziparchive import ZipArchive

Path = tech.fs.Path


class QueryCondition(Enum):
    BEAD_NAME = auto()
    KIND = auto()
    CONTENT_ID = auto()
    AT_TIME = auto()
    NEWER_THAN = auto()
    OLDER_THAN = auto()
    AT_OR_NEWER = auto()
    AT_OR_OLDER = auto()


# private and specific to Box implementation, when Box gains more power,
# it should change how it handles queries (e.g. using BEAD_NAME, KIND,
# or CONTENT_ID directly through an index)


_CHECKERS = {
    QueryCondition.BEAD_NAME: lambda name: lambda bead: bead.name == name,
    QueryCondition.KIND: lambda kind: lambda bead: bead.kind == kind,
    QueryCondition.CONTENT_ID: lambda prefix: lambda bead: bead.content_id.startswith(prefix),
    QueryCondition.AT_TIME: lambda timestamp: lambda bead: bead.freeze_time == timestamp,
    QueryCondition.NEWER_THAN: lambda timestamp: lambda bead: bead.freeze_time > timestamp,
    QueryCondition.OLDER_THAN: lambda timestamp: lambda bead: bead.freeze_time < timestamp,
    QueryCondition.AT_OR_NEWER: lambda timestamp: lambda bead: bead.freeze_time >= timestamp,
    QueryCondition.AT_OR_OLDER: lambda timestamp: lambda bead: bead.freeze_time <= timestamp,
}


def compile_conditions(conditions):
    '''
    Compile list of (check-type, check-param)-s into a match function.
    '''
    checkers = [_CHECKERS[check_type](check_param) for check_type, check_param in conditions]

    def match(bead):
        for check in checkers:
            if not check(bead):
                return False
        return True
    return match


ARCHIVE_COMMENT = '''
This file is a BEAD zip archive.

It is a normal zip file that stores a discrete computation of the form

    output = code(*inputs)

The archive contains

- inputs as part of metadata file: references (content_id) to other BEADs
- code   as files
- output as files
- extra metadata to support
  - linking different versions of the same computation
  - determining the newest version
  - reproducing multi-BEAD computation sequences built by a distributed team

There {is,will be,was} more info about BEADs at

- https://unknot.io
- https://github.com/ceumicrodata/bead
- https://github.com/e3krisztian/bead

----

'''


class BeadSearch(ABC):
    """
    Abstract base class for searching beads with a fluent builder pattern.
    """

    @abstractmethod
    def by_name(self, name):
        """Filter by bead name."""
        return self

    @abstractmethod
    def by_kind(self, kind):
        """Filter by bead kind."""
        return self

    @abstractmethod
    def by_content_id(self, content_id):
        """Filter by content ID."""
        return self

    @abstractmethod
    def at_time(self, timestamp):
        """Exact timestamp match."""
        return self

    @abstractmethod
    def newer_than(self, timestamp):
        """Timestamp > given value."""
        return self

    @abstractmethod
    def older_than(self, timestamp):
        """Timestamp < given value."""
        return self

    @abstractmethod
    def at_or_newer(self, timestamp):
        """Timestamp >= given value."""
        return self

    @abstractmethod
    def at_or_older(self, timestamp):
        """Timestamp <= given value."""
        return self

    @abstractmethod
    def unique(self):
        """Keep one instance by content_id."""
        return self

    @abstractmethod
    def first(self) -> Archive:
        """Return first bead found or raise LookupError if none found."""
        pass

    @abstractmethod
    def oldest(self) -> Archive:
        """Return oldest (by timestamp) matching bead or raise LookupError if none found."""
        pass

    @abstractmethod
    def newest(self) -> Archive:
        """Return newest (by timestamp) matching bead or raise LookupError if none found."""
        pass

    @abstractmethod
    def newer(self, n: int = 1) -> Archive:
        """Return nth newest bead (0=oldest, 1=2nd oldest, etc.) or raise LookupError if less items found."""
        pass

    @abstractmethod
    def older(self, n: int = 1) -> Archive:
        """Return nth oldest bead (0=newest, 1=2nd newest, etc.) or raise LookupError if less items found."""
        pass

    @abstractmethod
    def all(self) -> list[Archive]:
        """Return list of all matching beads."""
        pass


class BaseSearch(BeadSearch):
    """
    Base implementation of BeadSearch with common functionality.
    """

    def __init__(self):
        self.conditions = []
        self._unique_filter = False

    def by_name(self, name: str):
        if not name:
            raise ValueError("Name cannot be empty")
        self.conditions.append((QueryCondition.BEAD_NAME, name))
        return self

    def by_kind(self, kind: str):
        if not kind:
            raise ValueError("Kind cannot be empty")
        self.conditions.append((QueryCondition.KIND, kind))
        return self

    def by_content_id(self, content_id: str):
        if not content_id:
            raise ValueError("Content ID cannot be empty")
        self.conditions.append((QueryCondition.CONTENT_ID, content_id))
        return self

    def at_time(self, timestamp):
        if isinstance(timestamp, str):
            timestamp = time_from_timestamp(timestamp)
        self.conditions.append((QueryCondition.AT_TIME, timestamp))
        return self

    def newer_than(self, timestamp):
        if isinstance(timestamp, str):
            timestamp = time_from_timestamp(timestamp)
        self.conditions.append((QueryCondition.NEWER_THAN, timestamp))
        return self

    def older_than(self, timestamp):
        if isinstance(timestamp, str):
            timestamp = time_from_timestamp(timestamp)
        self.conditions.append((QueryCondition.OLDER_THAN, timestamp))
        return self

    def at_or_newer(self, timestamp):
        if isinstance(timestamp, str):
            timestamp = time_from_timestamp(timestamp)
        self.conditions.append((QueryCondition.AT_OR_NEWER, timestamp))
        return self

    def at_or_older(self, timestamp):
        if isinstance(timestamp, str):
            timestamp = time_from_timestamp(timestamp)
        self.conditions.append((QueryCondition.AT_OR_OLDER, timestamp))
        return self

    def unique(self):
        self._unique_filter = True
        return self

    def first(self) -> Archive:
        try:
            return next(iter(self))
        except StopIteration:
            raise LookupError("No beads found")

    def oldest(self) -> Archive:
        oldest_bead = None
        for bead in self:
            if oldest_bead is None or bead.freeze_time < oldest_bead.freeze_time:
                oldest_bead = bead
        if oldest_bead is None:
            raise LookupError("No beads found")
        return oldest_bead

    def newest(self) -> Archive:
        newest_bead = None
        for bead in self:
            if newest_bead is None or bead.freeze_time > newest_bead.freeze_time:
                newest_bead = bead
        if newest_bead is None:
            raise LookupError("No beads found")
        return newest_bead

    def newer(self, n: int = 1) -> Archive:
        beads = list(self)
        if not beads:
            raise LookupError("No beads found")
        sorted_beads = sorted(beads, key=lambda b: b.freeze_time)
        if n >= len(sorted_beads):
            raise LookupError(f"Not enough beads found (requested index {n}, found {len(sorted_beads)})")
        return sorted_beads[n]

    def older(self, n: int = 1) -> Archive:
        beads = list(self)
        if not beads:
            raise LookupError("No beads found")
        sorted_beads = sorted(beads, key=lambda b: b.freeze_time, reverse=True)
        if n >= len(sorted_beads):
            raise LookupError(f"Not enough beads found (requested index {n}, found {len(sorted_beads)})")
        return sorted_beads[n]

    def all(self) -> list[Archive]:
        return list(self)

    @abstractmethod
    def __iter__(self) -> Iterator[Archive]:
        pass


class FileBasedSearch(BaseSearch):
    """
    Concrete implementation of BeadSearch for file-based boxes.
    """

    def __init__(self, box):
        super().__init__()
        self.box = box

    def __iter__(self):
        beads = self.box._beads(self.conditions)
        if self._unique_filter:
            seen_content_ids = set()
            for bead in beads:
                if bead.content_id not in seen_content_ids:
                    seen_content_ids.add(bead.content_id)
                    yield bead
        else:
            yield from beads


class MultiBoxSearch(BaseSearch):
    """
    Search across multiple boxes.
    """

    def __init__(self, boxes):
        super().__init__()
        self.boxes = boxes

    def __iter__(self):
        if self._unique_filter:
            seen_content_ids = set()
            for box in self.boxes:
                try:
                    for bead in box._beads(self.conditions):
                        if bead.content_id not in seen_content_ids:
                            seen_content_ids.add(bead.content_id)
                            yield bead
                except (InvalidArchive, IOError, OSError):
                    continue
        else:
            for box in self.boxes:
                try:
                    yield from box._beads(self.conditions)
                except (InvalidArchive, IOError, OSError):
                    continue

    def first(self) -> Archive:
        try:
            return next(iter(self))
        except StopIteration:
            raise LookupError("No beads found")


def search_boxes(boxes):
    """
    Module-level convenience function that returns a MultiBoxSearch.
    """
    return MultiBoxSearch(boxes)


@dataclass
class Box:
    """
    Store Beads.
    """
    name: str
    location: Path

    @property
    def directory(self):
        '''
        Location as a Path.

        Valid only for local boxes.
        '''
        return Path(self.location)

    def all_beads(self) -> Iterator[Archive]:
        '''
        Iterator for all beads in this Box
        '''
        return iter(self._beads([]))

    def _beads(self, conditions) -> Iterable[Archive]:
        '''
        Retrieve matching beads.
        '''
        match = compile_conditions(conditions)

        bead_names = {
            value
            for tag, value in conditions
            if tag == QueryCondition.BEAD_NAME}
        if bead_names:
            if len(bead_names) > 1:
                # easy path: names disagree
                return []
            # beadname_20170615T075813302092+0200.zip
            glob = bead_names.pop() + '_????????T????????????[-+]????.zip'
        else:
            glob = '*'

        paths = self.directory.glob(glob)
        beads = self._archives_from(paths)
        candidates = (bead for bead in beads if match(bead))
        return candidates

    def _archives_from(self, paths: Iterable[Path]) -> Iterator[Archive]:
        for path in paths:
            try:
                archive = ZipArchive(path, self.name)
            except InvalidArchive:
                # TODO: log/report problem
                pass
            else:
                yield archive

    def store(self, workspace, freeze_time) -> Path:
        # -> Bead
        if not self.directory.exists():
            raise BoxError(f'Box "{self.name}": directory {self.directory} does not exist')
        if not self.directory.is_dir():
            raise BoxError(f'Box "{self.name}": {self.directory} is not a directory')
        zipfilename = (
            self.directory / f'{workspace.name}_{freeze_time}.zip')
        workspace.pack(zipfilename, freeze_time=freeze_time, comment=ARCHIVE_COMMENT)
        return zipfilename

    def search(self):
        """
        Return a FileBasedSearch instance for fluent search operations.
        """
        return FileBasedSearch(self)
