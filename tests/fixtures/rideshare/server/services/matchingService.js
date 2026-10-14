const Driver = require('../models/Driver');
exports.assignDriver = async (ride) => { const d = await Driver.findOne({ available: true }); };
