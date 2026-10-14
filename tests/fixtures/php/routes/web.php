<?php

use App\Http\Controllers\BillController;

Route::get('/bills', [BillController::class, 'index']);
