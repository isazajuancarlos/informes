import { Component, computed, inject, signal } from '@angular/core';
import { NavigationEnd, Router, RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatListModule } from '@angular/material/list';
import { MatMenuModule } from '@angular/material/menu';
import { MatSidenavModule } from '@angular/material/sidenav';
import { MatToolbarModule } from '@angular/material/toolbar';
import { filter } from 'rxjs/operators';

import { Sesion } from './nucleo/sesion';

interface Entrada {
  ruta: string;
  titulo: string;
  icono: string;
  /** Solo para administradores. Ver `menuVisible`. */
  soloAdmin?: boolean;
}

@Component({
  selector: 'app-root',
  imports: [
    RouterOutlet,
    RouterLink,
    RouterLinkActive,
    MatButtonModule,
    MatIconModule,
    MatListModule,
    MatMenuModule,
    MatSidenavModule,
    MatToolbarModule,
  ],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App {
  sesion = inject(Sesion);
  private router = inject(Router);

  readonly menu: Entrada[] = [
    { ruta: '/entregas', titulo: 'Entregas', icono: 'assignment_turned_in' },
    { ruta: '/notificaciones', titulo: 'Notificaciones', icono: 'notifications' },
    { ruta: '/estadisticas', titulo: 'Estadísticas', icono: 'insights' },
    { ruta: '/informes', titulo: 'Informes', icono: 'description' },
    { ruta: '/cronograma', titulo: 'Cronograma', icono: 'event_repeat' },
    { ruta: '/cuentas', titulo: 'Cuentas', icono: 'manage_accounts', soloAdmin: true },
  ];

  /**
   * Lo que se pinta en el lateral.
   *
   * Ocultar una entrada es COMODIDAD, no seguridad: quien edite su rol en memoria
   * la verá, y el servidor le seguirá contestando 403. La autorización vive en
   * `autz::Alcance` y en `cuentas::solo_admin`, en un solo sitio. Esto solo evita
   * enseñar una puerta que no se puede abrir.
   */
  readonly menuVisible = computed(() => {
    const admin = this.sesion.actor()?.rol === 'Admin';
    return this.menu.filter((e) => !e.soloAdmin || admin);
  });

  /** El lateral arranca abierto solo si hay sitio. En un portátil de 13" abierto
   *  por defecto deja la tabla de entregas sin ancho para sus seis columnas. */
  readonly anchaLaPantalla = signal(window.innerWidth >= 1100);

  private readonly ruta = signal(this.router.url);
  readonly tituloActual = computed(
    () => this.menu.find((e) => this.ruta().startsWith(e.ruta))?.titulo ?? 'SIGER',
  );

  /** Lo que falló al cerrar en todos los dispositivos, si falló. */
  readonly errorCuenta = signal('');

  constructor() {
    this.router.events
      .pipe(filter((e): e is NavigationEnd => e instanceof NavigationEnd))
      .subscribe((e) => this.ruta.set(e.urlAfterRedirects));
  }

  /**
   * «Cerrar sesión en todos los dispositivos» (#194).
   *
   * Si el servidor no pudo revocar, se queda aquí y lo dice. No se navega al
   * acceso: salir de la pantalla sería indistinguible del caso en que sí
   * funcionó, y esto se pulsa justo cuando saberlo importa.
   */
  async cerrarEnTodos() {
    this.errorCuenta.set('');
    try {
      await this.sesion.salirDeTodo();
    } catch (e) {
      this.errorCuenta.set((e as Error).message);
    }
  }
}
