"""Minimal native network-helper check for an installed cs705 wheel."""
import ipaddress
import sys
from pathlib import Path
import cs705
from cs705 import _native
from cs705.icf.settings import network as n


def rejects(exception, call):
    try:
        call()
    except exception:
        return
    raise AssertionError(f'expected {exception.__name__}')


def check():
    assert Path(cs705.__file__).is_relative_to(sys.prefix)
    assert Path(_native.__file__).is_relative_to(sys.prefix)
    for name in ('prefix_mask', 'correct_address', 'correct_gateway',
                 'Configuration', 'Effect', 'Field', 'parse_typed'):
        assert getattr(n, name) is getattr(_native.icf.settings.network, name)
    for prefix, mask in ((0, 0), (32, 0xffffffff), (33, 0x80000000), (255, 0xfffffffe)):
        assert n.prefix_mask(prefix) == mask
    ip = ipaddress.IPv4Address
    for address, prefix, expected in (
        ('192.0.2.0', 24, '192.0.2.1'),
        ('192.0.2.255', 24, '192.0.2.254'),
        ('0.0.0.0', 0, '0.0.0.255'),
    ):
        result = n.correct_address(ip(address), prefix)
        assert type(result) is ip and result == ip(expected)
    for address, gateway, expected in (
        ('192.0.2.1', '192.0.2.1', '192.0.2.2'),
        ('192.0.2.254', '192.0.2.255', '192.0.2.253'),
        ('192.0.2.1', '198.51.100.7', '192.0.2.7'),
        ('192.0.2.1', '0.0.0.0', '0.0.0.0'),
    ):
        result = n.correct_gateway(ip(address), ip(gateway), 24)
        assert type(result) is ip and result == ip(expected)
    for prefix, error in ((True, TypeError), ('24', TypeError), (-1, OverflowError), (256, OverflowError)):
        for call in (lambda: n.prefix_mask(prefix),
                     lambda: n.correct_address(ip('192.0.2.1'), prefix),
                     lambda: n.correct_gateway(ip('192.0.2.1'), ip('192.0.2.2'), prefix)):
            rejects(error, call)
    for value in ('192.0.2.1', 0xc0000201, True, ipaddress.IPv6Address('::1')):
        rejects(TypeError, lambda: n.correct_address(value, 24))
        rejects(TypeError, lambda: n.correct_gateway(value, ip('192.0.2.2'), 24))
        rejects(TypeError, lambda: n.correct_gateway(ip('192.0.2.1'), value, 24))
    print('Network check: OK (3 native helpers, prefix 0/32/33/255, IPv4 identity and strict inputs)')


if __name__ == '__main__':
    check()
