const router = require('express').Router();
const rideController = require('../controllers/rideController');
router.post('/', rideController.create);
router.get('/:id', rideController.show);
module.exports = router;
