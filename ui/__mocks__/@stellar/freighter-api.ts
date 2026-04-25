/** Reusable mock for @stellar/freighter-api across all UI tests. */

export const mockSignedXdr = 'SIGNED_XDR_MOCK_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==';

export const signTransaction = jest.fn(() =>
  Promise.resolve({ signedTxXdr: mockSignedXdr, signerAddress: 'GTEST123456789' })
);

export const getPublicKey = jest.fn(() =>
  Promise.resolve('GTEST123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJK')
);

export const isConnected = jest.fn(() => Promise.resolve(true));

export const requestAccess = jest.fn(() => Promise.resolve('GTEST123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJK'));

/** Reset all mocks to their default successful implementations. */
export function resetFreighterMocks() {
  signTransaction.mockReset().mockResolvedValue({ signedTxXdr: mockSignedXdr, signerAddress: 'GTEST123456789' });
  getPublicKey.mockReset().mockResolvedValue('GTEST123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJK');
  isConnected.mockReset().mockResolvedValue(true);
  requestAccess.mockReset().mockResolvedValue('GTEST123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ234567890ABCDEFGHIJK');
}
