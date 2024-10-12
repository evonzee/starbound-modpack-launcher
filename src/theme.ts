import { createTheme, ThemeOptions } from '@mui/material/styles';

export const themeOptions: ThemeOptions = {
	palette: {
	  mode: 'dark',
	  primary: {
		main: '#3f51b5',
	  },
	  secondary: {
		main: '#f50057',
	  },
	  background: {
		paper: '#271c1c',
	  },
	},
  };
export const theme = createTheme(themeOptions);