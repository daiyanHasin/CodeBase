from app import billing, models


def test_total():
    assert billing.total([models.Bill(2)]) == 2
