/** Reusable mock for @albedo-link/intent across all UI tests. */

const intent = {
  publicKey: jest.fn(() =>
    Promise.resolve({ pubkey: 'GALBEDO123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFG' })
  ),
  tx: jest.fn(() =>
    Promise.resolve({ signed_envelope_xdr: 'ALBEDO_SIGNED_XDR_MOCK' })
  ),
};

export default intent;
