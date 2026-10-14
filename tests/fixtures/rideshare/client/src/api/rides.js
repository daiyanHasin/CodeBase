import axios from 'axios';
export const requestRide = (from, to) => axios.post(`${API}/api/rides`, { from, to });
export const rideStatus = (id) => fetch(`/api/rides/${id}`);
