const matching = require('../services/matchingService');
const Ride = require('../models/Ride');
exports.create = async (req, res) => { const ride = await Ride.create(req.body); await matching.assignDriver(ride); };
exports.show = async (req, res) => {};
