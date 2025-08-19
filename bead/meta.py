'''
Keys for accessing fields in .BEAD_META structures

.BEAD_META files are persistent python dictionaries,
with the following minimum structure:

{
    meta_version: ...,
    inputs: {
        'nick1' : {
            kind: ...,
            content_id: ...,
            freeze_time: ...,
        },
        'nick2' : {
            kind: ...,
            content_id: ...,
            freeze_time: ...,
        },
        ...
    },
    kind: ...,
    freeze_time: ...,  # only archives - naive ordering
    freeze_name: ...,  # only archives, bead name for bootstrapping
}
'''

import attr

from .tech.timestamp import time_from_timestamp

# Metadata versions determine the content_id used and potentially
# other processing differences. Having it in the metadata potentially
# enables interoperability and backwards compatibility of BEADs.
# If the content_id ever needs to be changed/upgraded we still
# want existing BEADs to remain connected and alive.

META_VERSION = 'meta_version'
KIND = 'kind'
INPUTS = 'inputs'
INPUT_KIND         = 'kind'
INPUT_CONTENT_ID   = 'content_id'
INPUT_FREEZE_TIME  = 'freeze_time'


class ValidatingStr(str):
    def __init__(self, string: str = ''):
        if not self.is_wellformed(string):
            raise ValueError(f'Not a valid {self.__class__.__name__}: {string!r}')
        str.__init__(string)

    @classmethod
    def is_wellformed(cls, string: str) -> bool:
        raise NotImplementedError


class BeadName(ValidatingStr):
    @classmethod
    def is_wellformed(cls, string: str) -> bool:
        # check, that string can be a path in current directory
        return string not in ('', '.', '..') and '/' not in string and '__' not in string


assert type(BeadName('asd')) is BeadName
assert type(BeadName('asd') + '/') is not BeadName
assert type(BeadName('asd')[0]) is not BeadName


class InputName(BeadName):
    pass


assert isinstance(InputName('asd'), BeadName)


@attr.s(auto_attribs=True, frozen=True)
class InputSpec:
    name: InputName = attr.ib(converter=InputName)
    kind: str
    content_id: str
    freeze_time_str: str

    @property
    def freeze_time(self):
        return time_from_timestamp(self.freeze_time_str)


def parse_inputs(meta):
    '''
    Parse and yield input specification from meta as records.
    '''
    inputs = meta[INPUTS]
    for name in inputs:
        spec = inputs[name]
        yield InputSpec(
            name,
            spec[INPUT_KIND],
            spec[INPUT_CONTENT_ID],
            spec[INPUT_FREEZE_TIME])


# Archive meta:
FREEZE_TIME = 'freeze_time'
FREEZE_NAME = 'freeze_name'
