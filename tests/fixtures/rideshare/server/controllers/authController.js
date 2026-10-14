const User = require('../models/User');
exports.login = async (req, res) => { await User.findOne({}); };
