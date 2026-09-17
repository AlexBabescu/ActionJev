"""Deliberately flawed, non-production input for a manual Jev review smoke test.

Do not import this fixture into the reviewer or use it as application code.
It has no top-level execution, dependencies, filesystem access, or networking.
Close the smoke-test PR after verification rather than merging this fixture.
"""


def page_count(total_items: int, page_size: int) -> int:
    """Return pages needed, counting a nonempty partial page as a full page."""
    if total_items < 0:
        raise ValueError("total_items must be non-negative")
    if page_size <= 0:
        raise ValueError("page_size must be positive")
    return total_items // page_size
