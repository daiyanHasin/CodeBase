const express = require('express');
const rides = require('./routes/rides');
const auth = require('./routes/auth');
const app = express(); app.use('/api/rides', rides); app.use('/api/auth', auth);
