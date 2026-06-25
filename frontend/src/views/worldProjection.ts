const A1 = 1.340264;
const A2 = -0.081106;
const A3 = 0.000893;
const A4 = 0.003796;
const M = Math.sqrt(3) / 2;

function extents(): [number, number] {
  const maximumX = Math.PI / (M * A1);
  const theta = Math.asin(M);
  const thetaSquared = theta * theta;
  const thetaSixth = thetaSquared * thetaSquared * thetaSquared;
  const maximumY = theta * (A1 + A2 * thetaSquared + thetaSixth * (A3 + A4 * thetaSquared));
  return [maximumX, maximumY];
}

export function equalEarthForward(longitudeDegrees: number, latitudeDegrees: number): [number, number] {
  const longitude = Math.max(-180, Math.min(180, longitudeDegrees)) * Math.PI / 180;
  const latitude = Math.max(-90, Math.min(90, latitudeDegrees)) * Math.PI / 180;
  const theta = Math.asin(M * Math.sin(latitude));
  const thetaSquared = theta * theta;
  const thetaSixth = thetaSquared * thetaSquared * thetaSquared;
  const denominator = M * (
    A1 + 3 * A2 * thetaSquared + thetaSixth * (7 * A3 + 9 * A4 * thetaSquared)
  );
  return [
    longitude * Math.cos(theta) / denominator,
    theta * (A1 + A2 * thetaSquared + thetaSixth * (A3 + A4 * thetaSquared)),
  ];
}

export function equalEarthNormalized(longitude: number, latitude: number): [number, number] {
  const [x, y] = equalEarthForward(longitude, latitude);
  const [maximumX, maximumY] = extents();
  return [((x / maximumX) + 1) / 2, (1 - (y / maximumY)) / 2];
}

export function equalEarthInverseNormalized(
  normalizedX: number,
  normalizedY: number,
): [number, number] | null {
  const [maximumX, maximumY] = extents();
  const x = (normalizedX * 2 - 1) * maximumX;
  const y = (1 - normalizedY * 2) * maximumY;
  let theta = y / A1;
  for (let iteration = 0; iteration < 12; iteration += 1) {
    const thetaSquared = theta * theta;
    const thetaSixth = thetaSquared * thetaSquared * thetaSquared;
    const value = theta * (
      A1 + A2 * thetaSquared + thetaSixth * (A3 + A4 * thetaSquared)
    ) - y;
    const derivative = A1 + 3 * A2 * thetaSquared
      + thetaSixth * (7 * A3 + 9 * A4 * thetaSquared);
    const adjustment = value / derivative;
    theta -= adjustment;
    if (Math.abs(adjustment) < 1e-12) break;
  }
  const cosine = Math.cos(theta);
  if (Math.abs(cosine) < 1e-12) return null;
  const thetaSquared = theta * theta;
  const thetaSixth = thetaSquared * thetaSquared * thetaSquared;
  const longitude = x * M * (
    A1 + 3 * A2 * thetaSquared + thetaSixth * (7 * A3 + 9 * A4 * thetaSquared)
  ) / cosine;
  const latitude = Math.asin(Math.max(-1, Math.min(1, Math.sin(theta) / M)));
  if (Math.abs(longitude) > Math.PI * 1.01) return null;
  return [longitude * 180 / Math.PI, latitude * 180 / Math.PI];
}
