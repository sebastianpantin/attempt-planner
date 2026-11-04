const oneDecimal = new Intl.NumberFormat("en", { maximumFractionDigits: 1 });
const percentFormat = (digits: number) =>
  new Intl.NumberFormat("en", {
    style: "percent",
    maximumFractionDigits: digits,
  });
const onePercentDecimal = percentFormat(1);
const twoPercentDecimals = percentFormat(2);

export const kg = (value: number) => `${oneDecimal.format(value)} kg`;

export const percent = (probability: number) =>
  onePercentDecimal.format(probability);

export const kgWithError = (value: number, error: number) =>
  `${kg(value)} ± ${kg(error)}`;

export const finePercent = (probability: number) =>
  twoPercentDecimals.format(probability);

export const percentWithError = (probability: number, error: number) =>
  `${percent(probability)} ± ${finePercent(error)}`;
