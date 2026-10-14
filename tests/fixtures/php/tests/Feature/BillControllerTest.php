<?php

class BillControllerTest extends TestCase
{
    public function test_index() { $this->get('/bills')->assertOk(); }
}
