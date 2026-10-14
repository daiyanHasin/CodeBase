"""Adds up bills for a customer."""
from .models import Bill


def total(items):
    """Sum of all bill amounts."""
    return sum(x.amount for x in items)
