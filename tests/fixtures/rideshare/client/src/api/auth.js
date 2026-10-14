import axios from 'axios';
export const login = (u, p) => axios.post('/api/auth/login', { u, p });
