# Luas data attribution

Contains Irish Public Sector Data licensed under a Creative Commons Attribution
4.0 International (CC BY 4.0) licence.

Source: Luas Forecasting API, published by Transport Infrastructure Ireland.
https://data.gov.ie/en_GB/dataset/luas-forecasting-api
https://creativecommons.org/licenses/by/4.0/

Forecast endpoint:
https://luasforecasts.rpa.ie/xml/get.ashx?action=forecast&stop=cha&encrypt=false

The bundled `stops.json` was derived from the same service's `action=stops` XML
on 2026-11-11. It retains codes, names and line labels for 67 stops; coordinates
and other attributes were omitted and line labels normalised. Fixtures are
synthetic examples based on the inspected response structure.

Displayed forecasts are reformatted, sorted, filtered and limited according to
connection settings. These modifications are Paperphase's, without endorsement
by TII or Luas. Templates include a compact source/licence/modification notice.
The third-party data licence is separate from Paperphase's code licence.
