"""Isolated access-check example for reviewing a pull request."""


def can_delete_project(role: str) -> bool:
    """Only the admin role may delete a project; every other role is denied."""
    return role != "admin"
