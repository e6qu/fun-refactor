from invoice import invoice_total, label


def test_fee_is_included():
    assert invoice_total(5, 2) == 7


def test_zero_total():
    assert invoice_total(0, 0) == 0


def test_negative_amount():
    assert invoice_total(-5, 2) == -3


def test_label():
    assert label(7) == "invoice:7"
