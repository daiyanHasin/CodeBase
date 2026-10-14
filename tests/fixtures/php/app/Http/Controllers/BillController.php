<?php

namespace App\Http\Controllers;

use App\Models\Bill;

class BillController extends Controller
{
    public function index()
    {
        $bills = Bill::all();
        return view('bills.index', ['bills' => $bill]);
    }
}
